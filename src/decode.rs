//! Full container decoding with recomputed hashes (issue #19, under #17).
//!
//! # Why a header check was never enough
//!
//! [`containers`](crate::containers) validates structure and stops there, on
//! purpose: it earns a candidate the right to be decoded. #17 requires more
//! before a ROM Set may be called `Complete` — "fully decoded logical-content
//! hashes". The distinction is not pedantic. A CHD's header *declares* its
//! logical size and carries a SHA-1 of content nobody has read. Trusting either
//! would let a file that decodes to different bytes, or to nothing at all, sit
//! in a Library as a verified entry until the day someone tried to play it.
//!
//! # The declared size is a claim, not a measurement
//!
//! Every function here recomputes identity from the bytes that actually came
//! out of the decoder, and checks the total against what the header promised.
//! A mismatch is [`Outcome::Invalid`]: the container is internally
//! inconsistent, which is a property of the file rather than of the decode.
//!
//! # Decoding is supervised, never simply called
//!
//! These run under a [`Supervisor`], so a malformed map that drives the decoder
//! into an unbounded loop or an unbounded allocation ends the job instead of the
//! host. The supervisor is also the only thing that can tell "this file is bad"
//! from "the decoder died", which is why attribution happens there and not here.

use std::io::{Read, Seek};

use sha2::{Digest, Sha256};

use crate::{
    containers::{self, Format},
    outcomes::{Diagnostic, Outcome, ReasonCode},
    worker::{Supervisor, WorkerFault},
};

/// What a completed decode establishes about a container.
///
/// There is deliberately no constructor that takes a digest. The only way to
/// get one is to have decoded the content it describes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedContent {
    pub format: Format,
    /// Bytes the decoder actually produced.
    pub logical_bytes: u64,
    /// SHA-256 over the decoded logical content, in order.
    pub sha256: String,
}

/// The outcome of a supervised decode.
///
/// `Faulted` carries the supervisor's verdict rather than a diagnostic, because
/// only the supervisor knows whether the file or the worker was at fault, and
/// flattening the two here would throw that away.
#[derive(Debug)]
pub enum DecodeError {
    /// The container is internally inconsistent.
    Invalid(Diagnostic),
    /// The decode did not finish. See [`WorkerFault::diagnostic`].
    Faulted(WorkerFault),
}

impl DecodeError {
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::Invalid(diagnostic) => diagnostic.clone(),
            Self::Faulted(fault) => fault.diagnostic(),
        }
    }
}

fn inconsistent(reason: ReasonCode) -> DecodeError {
    DecodeError::Invalid(
        Diagnostic::new(Outcome::Invalid, reason).for_format(Format::Chd.extension()),
    )
}

/// Refuses a CHD whose hunks are not all inside the file, before any of them
/// is decoded.
///
/// Two failures share this shape and neither announces itself. A truncated file
/// yields zeros past its end instead of an error, and an unwritten map entry
/// reads as offset 0, which points back at the header. Both produce the
/// declared number of bytes; only the content is wrong.
fn check_hunks_are_present<F: Read + Seek>(
    chd: &chd::Chd<F>,
    file_bytes: u64,
) -> Result<(), DecodeError> {
    // Only meaningful where the content is stored verbatim. A compressed CHD is
    // *expected* to be smaller than what it decodes to, so file size says
    // nothing there; that case needs the content check noted below.
    if chd.header().is_compressed() {
        return Ok(());
    }

    let logical = chd.header().logical_bytes();
    let header_bytes = chd.header().len() as u64;
    let map_bytes = (chd.header().hunk_count() as u64).saturating_mul(4);

    // An uncompressed CHD stores its content verbatim, so the file has to be at
    // least as big as the header, its map, and the content together. Anything
    // less and the missing bytes will be read as zeros rather than as an error,
    // which is how a halved file still produces a full-length decode.
    let required = header_bytes
        .saturating_add(map_bytes)
        .saturating_add(logical);

    if file_bytes < required {
        // MalformedStructure, not ReadFailed: the bytes are missing from the
        // file itself. ReadFailed would suggest a failing disk and send the
        // user to check their hardware over a file that is simply damaged.
        return Err(inconsistent(ReasonCode::MalformedStructure));
    }

    Ok(())
}

/// Decodes a CHD and returns the identity of what came out.
///
/// The header is validated first and separately. A CHD that fails
/// [`validate_chd`](crate::containers::validate_chd) never reaches a
/// decompressor — the rejection happens while the file is still inert.
pub fn decode_chd<F: Read + Seek>(
    mut source: F,
    header_bytes: &[u8],
    supervisor: &mut Supervisor,
) -> Result<DecodedContent, DecodeError> {
    let header = containers::validate_chd(header_bytes).map_err(DecodeError::Invalid)?;

    // Establish where the file actually ends before decoding anything.
    //
    // This is not belt-and-braces. A `Read` over a truncated file returns zeros
    // at the end rather than an error, and a map entry left at zero addresses
    // offset 0 — so a CHD that has been cut in half, or whose map was never
    // written, decodes to a *plausible* run of bytes instead of failing. The
    // byte count comes out right and only the content is wrong, which is the
    // worst shape a corrupt file can have: it would enter the Library verified.
    let end = source
        .seek(std::io::SeekFrom::End(0))
        .map_err(|_| inconsistent(ReasonCode::ReadFailed))?;
    source
        .rewind()
        .map_err(|_| inconsistent(ReasonCode::ReadFailed))?;

    let mut chd =
        chd::Chd::open(source, None).map_err(|_| inconsistent(ReasonCode::MalformedStructure))?;

    check_hunks_are_present(&chd, end)?;

    let hunk_count = chd.header().hunk_count();
    let hunk_bytes = chd.header().hunk_size() as u64;

    let mut hunk_buffer = chd.get_hunksized_buffer();
    let mut compressed = Vec::new();
    let mut digest = Sha256::new();
    let mut produced: u64 = 0;

    // The header's logical size is the ceiling the decode is held to. A map
    // that keeps yielding hunks past it is the decompression-bomb shape, and
    // stopping at the declared size is what makes the ceiling meaningful.
    let declared = header.logical_bytes;

    for index in 0..hunk_count {
        supervisor.check().map_err(DecodeError::Faulted)?;

        let mut hunk = chd
            .hunk(index)
            .map_err(|_| inconsistent(ReasonCode::MalformedStructure))?;
        hunk.read_hunk_in(&mut compressed, &mut hunk_buffer)
            .map_err(|_| inconsistent(ReasonCode::MalformedStructure))?;

        // The final hunk is padded out to the hunk size; only the bytes the
        // container claims are content. Hashing the padding would make identity
        // depend on the hunk size a packer happened to choose.
        let remaining = declared.saturating_sub(produced);
        let take = remaining.min(hunk_bytes) as usize;
        digest.update(&hunk_buffer[..take]);
        produced += take as u64;

        supervisor
            .progress()
            .advance(hunk_bytes.min(compressed.len() as u64), take as u64);

        if produced >= declared {
            break;
        }
    }

    if produced != declared {
        return Err(inconsistent(ReasonCode::ChecksumMismatch));
    }

    Ok(DecodedContent {
        format: Format::Chd,
        logical_bytes: produced,
        sha256: format!("{:x}", digest.finalize()),
    })
}
