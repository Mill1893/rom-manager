//! Importing content into app-owned Library storage (issue #57).
//!
//! # The property this module exists to deliver
//!
//! **An imported ROM outlives its source.** After a successful import the
//! original file can be moved, renamed, deleted, or live on a drive that is
//! never plugged in again, and the ROM is still fully usable. The external path
//! is recorded as provenance — where these bytes were once seen — and is never
//! where the content lives.
//!
//! # Stage, verify, commit
//!
//! Import is three steps, in this order, per source object:
//!
//! 1. **Stage** — copy the bytes into app-owned storage under a temporary name,
//!    hashing as they are read.
//! 2. **Verify** — read the staged copy back and confirm it hashes to the same
//!    value. A copy that was not written correctly must never become Library
//!    content.
//! 3. **Commit** — move the staged file to its content-addressed home and record
//!    it, then record the origin observation.
//!
//! Each source object runs this independently, so **one failed candidate does
//! not roll back successful imports**. A partly-successful batch is the normal
//! outcome of importing a folder where one file is unreadable, and the user
//! keeps everything that worked.
//!
//! # Immutability
//!
//! A committed source object is never rewritten. Re-importing identical bytes
//! adds an origin observation and touches nothing else. A later mismatch is
//! corruption to be quarantined, never an update to apply — that is issue #61's
//! to implement; this slice establishes the invariant it depends on.

use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use crate::{Store, StoreError, sha256};

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("cannot read the source: {0}")]
    Source(String),
    #[error("app-owned storage could not be written: {0}")]
    Storage(String),
    #[error("the staged copy did not match what was read")]
    StagingMismatch,
    #[error("the archive could not be read: {0}")]
    Archive(String),
    #[error("an archive member escapes the archive root: {0}")]
    UnsafeMember(String),
    #[error("a materialized member did not reproduce its recorded identity")]
    MaterializationMismatch,
    #[error("the recovery candidate does not reproduce the recorded content identity")]
    RecoveryMismatch,
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// What one import attempt did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Imported {
    pub content_digest: String,
    pub size: u64,
    /// False when these exact bytes were already owned, so this import added
    /// provenance rather than content.
    pub stored_new_object: bool,
}

/// The root of app-owned Library storage.
pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, ImportError> {
        let root = root.into();
        fs::create_dir_all(root.join("objects"))
            .map_err(|error| ImportError::Storage(error.to_string()))?;
        fs::create_dir_all(root.join("staging"))
            .map_err(|error| ImportError::Storage(error.to_string()))?;
        Ok(Self { root })
    }

    /// Content-addressed home for a digest, fanned out so no directory grows
    /// without bound.
    fn object_path(&self, digest: &str) -> PathBuf {
        self.root
            .join("objects")
            .join(&digest[..2])
            .join(&digest[2..])
    }

    /// Imports one file, leaving the source untouched.
    ///
    /// `now` is supplied rather than read from the clock so imports are
    /// reproducible in tests.
    pub fn import_file(
        &self,
        store: &Store,
        source: &Path,
        now: i64,
    ) -> Result<Imported, ImportError> {
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ImportError::Source("the source has no usable file name".into()))?
            .to_owned();

        // 1. Stage — copy into app-owned storage, hashing what is actually read
        //    rather than trusting a separate pass over the source, which could
        //    see different bytes.
        let (staged, digest, size) = self.stage(source)?;

        // 2. Verify — the staged copy must reproduce the same digest. Written
        //    bytes that do not read back correctly never become Library
        //    content.
        let staged_digest = digest_of(&staged).map_err(|error| {
            let _ = fs::remove_file(&staged);
            ImportError::Storage(error.to_string())
        })?;
        if staged_digest != digest {
            let _ = fs::remove_file(&staged);
            return Err(ImportError::StagingMismatch);
        }

        // 3. Commit — move into place, then record. Recording after the move
        //    means durable state never names an object that is not there yet.
        let destination = self.object_path(&digest);
        let already_owned = destination.exists();
        if already_owned {
            // Identical bytes are already owned. The staged copy is redundant;
            // the existing object is immutable and is not touched.
            let _ = fs::remove_file(&staged);
        } else {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| ImportError::Storage(error.to_string()))?;
            }
            fs::rename(&staged, &destination)
                .map_err(|error| ImportError::Storage(error.to_string()))?;
        }

        let stored = self
            .object_path(&digest)
            .strip_prefix(&self.root)
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| digest.clone());

        store.record_source_object(&digest, size, &stored, now)?;
        store.record_origin_observation(&digest, &source.to_string_lossy(), &file_name, now)?;

        Ok(Imported {
            content_digest: digest,
            size,
            stored_new_object: !already_owned,
        })
    }

    /// Imports several files, continuing past failures.
    ///
    /// Returns one result per input, in order. A batch where some candidates
    /// fail is the normal outcome of scanning a folder, and everything that
    /// worked is kept.
    pub fn import_all(
        &self,
        store: &Store,
        sources: &[PathBuf],
        now: i64,
    ) -> Vec<Result<Imported, ImportError>> {
        sources
            .iter()
            .map(|source| self.import_file(store, source, now))
            .collect()
    }

    /// Reads the content of an owned object. This is what makes an import
    /// durable: it never consults the origin.
    pub fn read_object(&self, digest: &str) -> Result<Vec<u8>, ImportError> {
        fs::read(self.object_path(digest)).map_err(|error| ImportError::Storage(error.to_string()))
    }

    fn stage(&self, source: &Path) -> Result<(PathBuf, String, u64), ImportError> {
        let bytes = fs::read(source).map_err(|error| ImportError::Source(error.to_string()))?;
        let digest = sha256(&bytes);
        let staged = self.root.join("staging").join(&digest);
        fs::write(&staged, &bytes).map_err(|error| ImportError::Storage(error.to_string()))?;
        Ok((staged, digest, bytes.len() as u64))
    }
}

fn digest_of(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(sha256(&bytes))
}

/// What reading an archive found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Container {
    pub content_digest: String,
    /// `(member_path, content_digest, size)`, sorted by member path.
    pub members: Vec<(String, String, u64)>,
}

impl Library {
    /// Imports an archive as a Source Container.
    ///
    /// The archive is stored **byte-for-byte** like any other source object —
    /// its ROMs are derived materializations reproduced from it on demand, not
    /// separate Library content. Two archives packaging identical ROMs stay two
    /// distinct containers, because what was imported is the archive.
    ///
    /// A structurally malformed archive is **reported**, never imported as
    /// opaque complete content: an archive this application cannot read is not
    /// a ROM it can claim to hold.
    pub fn import_archive(
        &self,
        store: &Store,
        source: &Path,
        now: i64,
    ) -> Result<Container, ImportError> {
        // Read it before importing anything. A file that does not parse must
        // not leave a half-claimed container behind.
        let members = read_zip_members(source)?;

        let imported = self.import_file(store, source, now)?;
        store.record_container(&imported.content_digest, "zip", &members, now)?;

        Ok(Container {
            content_digest: imported.content_digest,
            members,
        })
    }

    /// Reproduces one member's bytes from its container.
    ///
    /// This is a derived materialization: the bytes are not stored, they are
    /// produced from content the application owns, and verified against the
    /// identity recorded when the container was read.
    pub fn materialize_member(
        &self,
        container_digest: &str,
        member_path: &str,
        expected_digest: &str,
    ) -> Result<Vec<u8>, ImportError> {
        let archive_bytes = self.read_object(container_digest)?;
        let cursor = io::Cursor::new(archive_bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|error| ImportError::Archive(error.to_string()))?;
        let mut entry = archive
            .by_name(member_path)
            .map_err(|error| ImportError::Archive(error.to_string()))?;
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| ImportError::Archive(error.to_string()))?;

        // Verified, not trusted: a materialization that does not reproduce the
        // recorded identity is not the ROM that was imported.
        if sha256(&bytes) != expected_digest {
            return Err(ImportError::MaterializationMismatch);
        }
        Ok(bytes)
    }
}

/// Reads a ZIP's members without extracting anything to disk.
///
/// Directory entries are skipped — they are structure, not content. A member
/// whose name escapes the archive root is refused outright rather than
/// sanitized, on the same footing as the target-path namespace rules.
fn read_zip_members(source: &Path) -> Result<Vec<(String, String, u64)>, ImportError> {
    let file = fs::File::open(source).map_err(|error| ImportError::Source(error.to_string()))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| ImportError::Archive(error.to_string()))?;

    let mut members = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ImportError::Archive(error.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        // `enclosed_name` is None when the member name escapes the archive
        // root. Refused, never repaired.
        let name = entry
            .enclosed_name()
            .ok_or_else(|| ImportError::UnsafeMember(entry.name().to_owned()))?
            .to_string_lossy()
            .replace('\\', "/");

        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| ImportError::Archive(error.to_string()))?;
        members.push((name, sha256(&bytes), bytes.len() as u64));
    }
    members.sort();
    Ok(members)
}

/// What a verification pass found.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IntegrityReport {
    pub verified: Vec<String>,
    /// Objects whose bytes are not what was stored. Quarantined, never
    /// overwritten.
    pub quarantined: Vec<String>,
    /// Objects the store names but whose bytes are gone entirely.
    pub missing: Vec<String>,
}

impl IntegrityReport {
    pub fn is_clean(&self) -> bool {
        self.quarantined.is_empty() && self.missing.is_empty()
    }
}

impl Library {
    /// Verifies one owned object against the digest it is filed under.
    ///
    /// A mismatch is **corruption, never an update**. The recorded digest is
    /// what the object *is*, so bytes that disagree are the thing that is
    /// wrong: they are quarantined and the object is marked unhealthy, rather
    /// than the record being rewritten to match whatever is now on disk.
    pub fn verify_object(
        &self,
        store: &Store,
        digest: &str,
        now: i64,
    ) -> Result<bool, ImportError> {
        let path = self.object_path(digest);
        let Ok(bytes) = fs::read(&path) else {
            store.quarantine_object(digest)?;
            return Ok(false);
        };
        if sha256(&bytes) == digest {
            store.record_verification(digest, now)?;
            return Ok(true);
        }

        // Move the unexpected bytes aside rather than deleting them. They are
        // evidence, and they may be the user's only copy of something.
        let quarantine = self.root.join("quarantine");
        let _ = fs::create_dir_all(&quarantine);
        let _ = fs::rename(&path, quarantine.join(digest));
        store.quarantine_object(digest)?;
        Ok(false)
    }

    /// A full, user-initiated integrity check across every owned object.
    pub fn verify_all(&self, store: &Store, now: i64) -> Result<IntegrityReport, ImportError> {
        let mut report = IntegrityReport::default();
        for digest in store.owned_objects()? {
            let present = self.object_path(&digest).exists();
            if self.verify_object(store, &digest, now)? {
                report.verified.push(digest);
            } else if present {
                report.quarantined.push(digest);
            } else {
                report.missing.push(digest);
            }
        }
        Ok(report)
    }

    /// Recovers a quarantined object from a strongly matching reimport.
    ///
    /// The candidate must reproduce the recorded digest exactly. Recovery from
    /// something merely similar would be indistinguishable from accepting the
    /// corruption.
    pub fn recover_object(
        &self,
        store: &Store,
        digest: &str,
        candidate: &Path,
        now: i64,
    ) -> Result<(), ImportError> {
        let bytes = fs::read(candidate).map_err(|error| ImportError::Source(error.to_string()))?;
        if sha256(&bytes) != digest {
            return Err(ImportError::RecoveryMismatch);
        }
        let destination = self.object_path(digest);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| ImportError::Storage(error.to_string()))?;
        }
        fs::write(&destination, &bytes).map_err(|error| ImportError::Storage(error.to_string()))?;
        store.restore_object(digest, now)?;
        Ok(())
    }
}

/// Why a candidate was passed over during a scan.
///
/// Skipped candidates are *reported*, never silently dropped: a user who
/// expected a file to import needs to know it was seen and why it was not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Skipped {
    /// An indirection. Never followed — a scan must not be steerable out of the
    /// folder the user pointed at.
    Indirection(String),
    /// A name the application cannot represent.
    Unsafe(String),
    /// Present but could not be read.
    Unreadable(String),
}

/// What a scan found.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanReport {
    /// Files present that are not yet known content.
    pub new_candidates: Vec<PathBuf>,
    /// Paths whose bytes are already owned, unchanged.
    pub unchanged: Vec<PathBuf>,
    /// Paths whose bytes differ from what was last seen there. A **new**
    /// candidate, never an update to the existing object.
    pub changed: Vec<PathBuf>,
    /// Observations that are no longer findable where they were recorded.
    pub now_unavailable: Vec<String>,
    pub skipped: Vec<Skipped>,
}

impl Library {
    /// Scans one Import Folder. Called only when the user asks.
    ///
    /// Reconciles provenance **without mutating Library content**: a moved
    /// input is recognized by content identity, a vanished one is marked
    /// unavailable, and changed bytes at a known path become a new candidate
    /// rather than an update. Nothing here imports anything.
    pub fn scan_folder(
        &self,
        store: &Store,
        folder: &Path,
        now: i64,
    ) -> Result<ScanReport, ImportError> {
        let mut report = ScanReport::default();
        let mut seen = Vec::new();
        walk(folder, &mut report, &mut seen)?;

        for path in seen {
            let display = path.to_string_lossy().into_owned();
            let Ok(bytes) = fs::read(&path) else {
                report.skipped.push(Skipped::Unreadable(display));
                continue;
            };
            let digest = sha256(&bytes);

            match store.observation_at(&display)? {
                // Same bytes, same place: nothing to reconcile.
                Some(known) if known == digest => report.unchanged.push(path),
                // Different bytes at a known path. The existing object is
                // immutable, so this is a new candidate — never an update.
                Some(known) => {
                    store.mark_observation_unavailable(&known, &display, now)?;
                    report.changed.push(path);
                }
                None => {
                    if store.source_object(&digest)?.is_some() {
                        // Already-owned content seen somewhere new: a moved
                        // input, matched by strong identity rather than path.
                        store.record_origin_observation(
                            &digest,
                            &display,
                            &path
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            now,
                        )?;
                        report.unchanged.push(path);
                    } else {
                        report.new_candidates.push(path);
                    }
                }
            }
        }

        // Anything recorded under this folder that the scan did not find is no
        // longer where it was seen.
        let prefix = folder.to_string_lossy().into_owned();
        for digest in store.owned_objects()? {
            for observed in store.available_observations(&digest)? {
                if observed.starts_with(&prefix) && !Path::new(&observed).exists() {
                    store.mark_observation_unavailable(&digest, &observed, now)?;
                    report.now_unavailable.push(observed);
                }
            }
        }

        report.new_candidates.sort();
        report.unchanged.sort();
        report.changed.sort();
        report.now_unavailable.sort();
        Ok(report)
    }
}

/// Recurses through ordinary directories, never following indirection.
///
/// A symlink is reported and stepped over rather than traversed, so a scan
/// cannot be steered outside the folder the user pointed at.
fn walk(
    directory: &Path,
    report: &mut ScanReport,
    seen: &mut Vec<PathBuf>,
) -> Result<(), ImportError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => {
            report.skipped.push(Skipped::Unreadable(
                directory.to_string_lossy().into_owned(),
            ));
            return Ok(());
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            report
                .skipped
                .push(Skipped::Unreadable(path.to_string_lossy().into_owned()));
            continue;
        };
        if kind.is_symlink() {
            report
                .skipped
                .push(Skipped::Indirection(path.to_string_lossy().into_owned()));
            continue;
        }
        if kind.is_dir() {
            walk(&path, report, seen)?;
        } else if path.file_name().and_then(|name| name.to_str()).is_none() {
            report
                .skipped
                .push(Skipped::Unsafe(path.to_string_lossy().into_owned()));
        } else {
            seen.push(path);
        }
    }
    Ok(())
}

/// Whether a ROM Set can be synced, and if not, which kind of problem it is.
///
/// These are three genuinely different situations and are never collapsed:
/// *incomplete* means the set is missing structure or membership; *unavailable*
/// means the set is whole but something it needs cannot be reproduced; and
/// *available* means its complete dependency closure can be materialized right
/// now.
///
/// A user can act on each differently — find the missing ROM, reconnect the
/// drive it came from, or just sync — which is exactly why one "not ready"
/// state would be useless.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetState {
    Incomplete,
    Unavailable,
    Available,
}

/// A ROM Set's state together with the specific reasons behind it.
///
/// Reasons are kept separately from the state so a state can be acted on
/// programmatically while the explanation stays specific enough to be useful.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetAvailability {
    pub state: SetState,
    /// Members the set expects but does not have.
    pub missing_members: Vec<String>,
    /// Members present but not reproducible — no healthy occurrence.
    pub unreproducible_members: Vec<String>,
}

impl SetAvailability {
    pub fn is_syncable(&self) -> bool {
        self.state == SetState::Available
    }
}

impl Library {
    /// Whether a single ROM can be reproduced right now.
    ///
    /// True only when a **healthy managed** occurrence exists. A cached copy is
    /// deliberately not consulted: the cache is disposable, so letting it
    /// establish availability would mean clearing the cache could make content
    /// unavailable.
    pub fn rom_is_available(&self, store: &Store, digest: &str) -> Result<bool, ImportError> {
        // Owned directly as its own source object.
        if store.object_is_healthy(digest)? && self.object_path(digest).exists() {
            return Ok(true);
        }
        // Or reproducible from a healthy container that holds it. A ROM inside
        // an archive has no source object of its own — the archive is what was
        // imported — so its health is the container's.
        for container in store.containers_holding(digest)? {
            if store.object_is_healthy(&container)? && self.object_path(&container).exists() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Classifies a ROM Set from its expected membership and dependency
    /// closure.
    ///
    /// `expected` is what the set should contain; `present` is what it actually
    /// has. Both are content digests.
    pub fn set_availability(
        &self,
        store: &Store,
        expected: &[String],
        present: &[String],
        dependencies: &[String],
    ) -> Result<SetAvailability, ImportError> {
        let missing_members: Vec<String> = expected
            .iter()
            .filter(|digest| !present.contains(digest))
            .cloned()
            .collect();

        // Structure first: a set that is not whole cannot be judged on whether
        // its parts are reproducible.
        if !missing_members.is_empty() {
            return Ok(SetAvailability {
                state: SetState::Incomplete,
                missing_members,
                unreproducible_members: Vec::new(),
            });
        }

        // The set is whole. Can everything it needs — members *and* the
        // dependency closure — actually be produced?
        let mut unreproducible_members = Vec::new();
        for digest in present.iter().chain(dependencies.iter()) {
            if !self.rom_is_available(store, digest)? {
                unreproducible_members.push(digest.clone());
            }
        }

        Ok(SetAvailability {
            state: if unreproducible_members.is_empty() {
                SetState::Available
            } else {
                SetState::Unavailable
            },
            missing_members: Vec::new(),
            unreproducible_members,
        })
    }
}
