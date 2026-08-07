//! Turning a scanned Import Folder into ROM Sets and a ROM Pack (issue #22).
//!
//! # The gap this closes
//!
//! [`Library::scan_folder`](crate::Library::scan_folder) reports what it found;
//! [`Library::import_file`](crate::Library::import_file) takes ownership of one
//! file's bytes. Neither produces anything a user can *sync*, because a Sync
//! Plan is built from ROM Sets in a ROM Pack, and nothing was creating either.
//! So the application could remember a folder, read it, and still offer nothing
//! to put on a device.
//!
//! # Identity comes from content, so scanning twice is not importing twice
//!
//! A ROM Set's identifier is derived from its content digest. Re-scanning the
//! same folder — or scanning a copy of it somewhere else — resolves to the same
//! ROM Set rather than a second one that happens to look identical. A
//! path-derived identifier would make a renamed file into a new game, and a
//! moved folder into a duplicate library.
//!
//! # What is refused
//!
//! Two cases are refused rather than guessed, and they are different:
//!
//! - An extension this release does not accept is `Unsupported`. The file is
//!   simply not something the application handles.
//! - An extension that several Platforms share — `.iso` is PlayStation 2 *and*
//!   PSP, `.chd` is four systems — is `NeedsPlatform`. The file is fine and the
//!   application cannot tell which system it is for. Picking the first match
//!   would silently file a PSP game under PlayStation 2, and the user would
//!   discover it when their handheld refused to launch it.
//!
//! A bare `.bin` is in neither category: it is refused because it is a *track*,
//! never a game, and only a descriptor may claim it.

use std::path::{Path, PathBuf};

use crate::{
    Library, Store,
    formats::{BASELINE, Representation, Support, may_stand_alone},
    library::ImportError,
    outcomes::{Diagnostic, Location, Outcome, ReasonCode},
    sha256,
};

#[derive(Debug, thiserror::Error)]
pub enum IntakeError {
    #[error("the folder could not be read: {0}")]
    Scan(String),
    #[error("durable state could not be written: {0}")]
    Store(String),
}

impl From<ImportError> for IntakeError {
    fn from(error: ImportError) -> Self {
        Self::Scan(error.to_string())
    }
}

impl From<crate::StoreError> for IntakeError {
    fn from(error: crate::StoreError) -> Self {
        Self::Store(error.to_string())
    }
}

/// What taking a folder in produced.
#[derive(Debug, Default)]
pub struct IntakeReport {
    /// ROM Sets now in the Library, in the order encountered.
    pub rom_sets: Vec<RomSetSummary>,
    /// Files the application declined to take in, each with its reason.
    pub declined: Vec<Diagnostic>,
    /// The ROM Pack holding everything above, when there was anything.
    pub pack: Option<(String, u32)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RomSetSummary {
    pub rom_set_id: String,
    pub title: String,
    pub platform: String,
    pub content_digest: String,
    /// False when these exact bytes were already owned.
    pub newly_stored: bool,
}

/// Which Platform an extension belongs to, when exactly one claims it.
enum PlatformMatch {
    One(&'static str),
    /// Several Platforms use this extension, so the file is fine and its
    /// Platform is undetermined.
    Several,
    None,
}

fn platform_for(extension: &str) -> PlatformMatch {
    let extension = extension.to_ascii_lowercase();
    let mut platforms: Vec<&'static str> = BASELINE
        .iter()
        .filter(|form| {
            form.extension.eq_ignore_ascii_case(&extension)
                && form.support != Support::Unsupported
                // Only a whole-file ROM stands alone. A descriptor or playlist
                // names other files, and taking one in without them would
                // produce a set that cannot materialize.
                && form.representation == Representation::SingleFile
        })
        .map(|form| form.platform)
        .collect();
    platforms.sort_unstable();
    platforms.dedup();

    match platforms.len() {
        0 => PlatformMatch::None,
        1 => PlatformMatch::One(platforms[0]),
        _ => PlatformMatch::Several,
    }
}

/// The title shown for a file, from its name with the extension removed.
///
/// Underscores become spaces because dumps are commonly named that way, and a
/// library listing "Super_Mario_Bros" reads as a filename rather than a game.
/// Nothing else is rewritten: bracketed region and revision tags are left
/// alone, because stripping them is a judgement about which copy this is, and
/// that belongs to metadata work rather than to reading a directory.
fn title_from(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().replace('_', " "))
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
        .trim()
        .to_owned()
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|extension| format!(".{}", extension.to_string_lossy().to_ascii_lowercase()))
        .unwrap_or_default()
}

fn declined(path: &Path, outcome: Outcome, reason: ReasonCode) -> Diagnostic {
    Diagnostic::new(outcome, reason).at(Location::in_source(path.to_string_lossy().into_owned()))
}

/// Scans `folder`, takes in every recognized standalone ROM, and gathers the
/// result into a ROM Pack named for the folder.
///
/// Nothing outside `folder` is read, and nothing in it is modified — import
/// copies bytes into Library storage and leaves the original where it was.
pub fn take_in(
    library: &Library,
    store: &Store,
    folder: &Path,
    now: i64,
) -> Result<IntakeReport, IntakeError> {
    let scan = library.scan_folder(store, folder, now)?;
    let mut report = IntakeReport::default();
    // Everything the folder currently holds, whether or not this scan is what
    // first saw it. The pack must describe the folder as it *is*; building it
    // from only the newly-seen files would mean adding one game to a folder
    // produced a pack containing exactly that game, and syncing it would then
    // remove every other game the user had already put on the device.
    let mut in_pack: Vec<RomSetSummary> = Vec::new();

    // Content already owned and unchanged where it sits. No bytes are re-read:
    // the digest recorded for that path is what identifies it.
    for path in &scan.unchanged {
        let display = path.to_string_lossy().into_owned();
        let Some(digest) = store.observation_at(&display)? else {
            continue;
        };
        let Some(rom_set_id) = store.rom_sets_using(&digest)?.into_iter().next() else {
            // Owned bytes that never became a ROM Set — a track claimed by a
            // descriptor, or something declined on an earlier pass.
            continue;
        };
        in_pack.push(RomSetSummary {
            rom_set_id,
            title: title_from(path),
            platform: match platform_for(&extension_of(path)) {
                PlatformMatch::One(platform) => platform.to_owned(),
                _ => String::new(),
            },
            content_digest: digest,
            newly_stored: false,
        });
    }

    // Content never seen before, and content whose bytes changed at a known
    // path. The second is a *new* candidate rather than an update, because the
    // object already owned is immutable.
    let candidates: Vec<PathBuf> = scan
        .new_candidates
        .into_iter()
        .chain(scan.changed)
        .collect();

    for path in candidates {
        let extension = extension_of(&path);

        if !may_stand_alone(&extension) {
            // A track, not a game. Only a descriptor may claim it.
            report.declined.push(declined(
                &path,
                Outcome::Ambiguous,
                ReasonCode::UnclassifiedMember,
            ));
            continue;
        }

        let platform = match platform_for(&extension) {
            PlatformMatch::One(platform) => platform,
            PlatformMatch::Several => {
                report.declined.push(declined(
                    &path,
                    Outcome::NeedsPlatform,
                    ReasonCode::PlatformUndetermined,
                ));
                continue;
            }
            PlatformMatch::None => {
                report.declined.push(declined(
                    &path,
                    Outcome::Unsupported,
                    ReasonCode::UnknownExtension,
                ));
                continue;
            }
        };

        let imported = match library.import_file(store, &path, now) {
            Ok(imported) => imported,
            Err(error) => {
                // Reading failed. That is the host's problem or the file's, and
                // either way it is not a claim about the content's shape.
                report.declined.push(
                    declined(&path, Outcome::IoFailure, ReasonCode::ReadFailed)
                        .for_format(error.to_string()),
                );
                continue;
            }
        };

        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let title = title_from(&path);

        // Every identifier is derived from the content digest, so the same
        // bytes always resolve to the same ROM Set however they were reached.
        let short = &imported.content_digest[..16];
        let rom_set_id = format!("rom-set-{short}");
        let game_id = format!(
            "game-{}",
            &sha256(format!("{platform}:{title}").as_bytes())[..16]
        );
        let release_id = format!("release-{short}");

        store.upsert_rom_set(
            (game_id.as_str(), platform, title.as_str()),
            (release_id.as_str(), "Unknown"),
            (
                rom_set_id.as_str(),
                imported.content_digest.as_str(),
                file_name.as_str(),
                imported.size,
            ),
        )?;
        store.record_origin_observation(
            &imported.content_digest,
            &path.to_string_lossy(),
            &file_name,
            now,
        )?;

        let summary = RomSetSummary {
            rom_set_id,
            title,
            platform: platform.to_owned(),
            content_digest: imported.content_digest,
            newly_stored: imported.stored_new_object,
        };
        in_pack.push(summary.clone());
        // `rom_sets` reports what *this* scan took in, which is what the user
        // is told about. The pack is built from the wider set above.
        report.rom_sets.push(summary);
    }

    if !in_pack.is_empty() {
        report.pack = Some(gather_into_pack(store, folder, &in_pack)?);
    }
    Ok(report)
}

/// Records the ROM Sets as a revision of the folder's ROM Pack.
///
/// A revision is minted only when the selection actually differs from the
/// newest one. Re-scanning an unchanged folder must not produce a revision that
/// says nothing, because every revision is something the user has to choose
/// between later.
fn gather_into_pack(
    store: &Store,
    folder: &Path,
    rom_sets: &[RomSetSummary],
) -> Result<(String, u32), IntakeError> {
    let title = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder.to_string_lossy().into_owned());
    let rom_pack_id = format!(
        "pack-{}",
        &sha256(folder.to_string_lossy().as_bytes())[..16]
    );

    let mut selection: Vec<(&str, &str)> = rom_sets
        .iter()
        .map(|set| (set.rom_set_id.as_str(), set.content_digest.as_str()))
        .collect();
    selection.sort_unstable();
    selection.dedup();

    let newest = store
        .rom_packs()?
        .into_iter()
        .filter(|pack| pack.rom_pack_id == rom_pack_id)
        .map(|pack| pack.revision)
        .max();

    if let Some(revision) = newest {
        let existing = store.pack_selection(&rom_pack_id, revision)?;
        let mut existing: Vec<(String, String)> = existing;
        existing.sort_unstable();
        let same = existing.len() == selection.len()
            && existing
                .iter()
                .zip(&selection)
                .all(|((id, digest), (new_id, new_digest))| id == new_id && digest == new_digest);
        if same {
            return Ok((rom_pack_id, revision));
        }
    }

    let revision = newest.unwrap_or(0) + 1;
    store.record_pack_selection(&rom_pack_id, revision, &selection)?;
    store.set_pack_title(&rom_pack_id, revision, &title)?;
    Ok((rom_pack_id, revision))
}
