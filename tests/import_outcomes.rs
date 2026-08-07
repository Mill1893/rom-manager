//! The typed outcome and compatibility-manifest contract (issue #19, under #17).

use rom_manager::{Diagnostic, LIMITS, Location, Outcome, ReasonCode, manifest};

const ALL_OUTCOMES: [Outcome; 10] = [
    Outcome::Complete,
    Outcome::Incomplete,
    Outcome::NeedsPlatform,
    Outcome::Ambiguous,
    Outcome::Unsupported,
    Outcome::Invalid,
    Outcome::LimitExceeded,
    Outcome::IoFailure,
    Outcome::ParserFailure,
    Outcome::Cancelled,
];

#[test]
fn exactly_one_outcome_is_rom_pack_eligible() {
    // #17: "Only `complete` is ROM Pack eligible." A ROM Set that copies onto a
    // device and does not run is worse than one that never copied, because the
    // failure surfaces later and away from any explanation.
    let eligible: Vec<_> = ALL_OUTCOMES
        .iter()
        .filter(|outcome| outcome.is_rom_pack_eligible())
        .collect();
    assert_eq!(eligible, vec![&Outcome::Complete]);
}

#[test]
fn no_failure_or_unsupported_input_can_enter_a_rom_pack() {
    for outcome in ALL_OUTCOMES {
        if outcome == Outcome::Complete {
            continue;
        }
        assert!(
            !outcome.is_rom_pack_eligible(),
            "{outcome:?} must never become an opaque complete ROM Set"
        );
    }
}

#[test]
fn an_incomplete_set_is_kept_but_stays_ineligible() {
    // Discarding it would lose the identification work and leave the user
    // nothing to add the missing member to.
    assert!(Outcome::Incomplete.enters_the_library());
    assert!(!Outcome::Incomplete.is_rom_pack_eligible());
}

#[test]
fn only_complete_and_incomplete_reach_the_library() {
    for outcome in ALL_OUTCOMES {
        let expected = matches!(outcome, Outcome::Complete | Outcome::Incomplete);
        assert_eq!(outcome.enters_the_library(), expected, "{outcome:?}");
    }
}

#[test]
fn our_own_failures_are_never_blamed_on_the_users_file() {
    // #17: "a worker fault is never reported as malformed user input."
    // Reporting a crashed decoder as Invalid sends the user to re-dump a disc
    // that was fine.
    for ours in [
        Outcome::ParserFailure,
        Outcome::IoFailure,
        Outcome::Cancelled,
    ] {
        assert!(
            !ours.blames_the_input(),
            "{ours:?} is our failure or the host's, not a defect in the file"
        );
    }
    for theirs in [
        Outcome::Invalid,
        Outcome::Unsupported,
        Outcome::Ambiguous,
        Outcome::LimitExceeded,
        Outcome::NeedsPlatform,
    ] {
        assert!(theirs.blames_the_input(), "{theirs:?} describes the input");
    }
}

#[test]
fn a_worker_failure_says_it_is_our_defect() {
    let remedy = ReasonCode::WorkerFailed.remediation();
    assert!(
        remedy.contains("not in your file"),
        "the user must not be sent to re-dump a disc over our bug: {remedy}"
    );
}

#[test]
fn every_reason_code_has_a_stable_spelling_and_a_remedy() {
    // The reason strings are part of the application's contract: they appear
    // in reports and may be matched on.
    let codes = [
        ReasonCode::SignatureMismatch,
        ReasonCode::UnknownExtension,
        ReasonCode::UnsupportedVersion,
        ReasonCode::UnsupportedMethod,
        ReasonCode::UnsupportedDirective,
        ReasonCode::EncryptionPresent,
        ReasonCode::ExternalKeyRequired,
        ReasonCode::ParentReferenceRequired,
        ReasonCode::NestedContainer,
        ReasonCode::SplitVolume,
        ReasonCode::TrailingPayload,
        ReasonCode::DuplicateNormalizedPath,
        ReasonCode::EscapingReference,
        ReasonCode::MissingMember,
        ReasonCode::NoMembers,
        ReasonCode::AmbiguousMembership,
        ReasonCode::UnclassifiedMember,
        ReasonCode::ChecksumMismatch,
        ReasonCode::MalformedStructure,
        ReasonCode::TrackOverlap,
        ReasonCode::TrackAlignment,
        ReasonCode::NonMonotonicLba,
        ReasonCode::PlatformUndetermined,
        ReasonCode::LimitExceeded,
        ReasonCode::RatioExceeded,
        ReasonCode::ReadFailed,
        ReasonCode::WorkerFailed,
        ReasonCode::OperationCancelled,
    ];
    let mut seen = Vec::new();
    for code in codes {
        let spelling = code.as_str();
        assert!(
            spelling.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{spelling} is not a stable snake_case code"
        );
        assert!(!seen.contains(&spelling), "{spelling} is duplicated");
        seen.push(spelling);
        assert!(
            code.remediation().len() > 20,
            "a code with no remedy leaves the user stuck: {spelling}"
        );
    }
}

#[test]
fn a_diagnostic_carries_the_location_and_both_sides_of_a_limit() {
    // "Too large" without the numbers is unactionable: the user cannot tell
    // whether the file is marginally over or actually hostile.
    let diagnostic = Diagnostic::new(Outcome::LimitExceeded, ReasonCode::LimitExceeded)
        .for_format("zip")
        .at(Location::in_source("games.zip")
            .within("disc.bin")
            .at_line(12)
            .at_track(3))
        .measured(1024, 99_999);

    assert_eq!(diagnostic.location.source.as_deref(), Some("games.zip"));
    assert_eq!(diagnostic.location.member.as_deref(), Some("disc.bin"));
    assert_eq!(diagnostic.location.line, Some(12));
    assert_eq!(diagnostic.location.track, Some(3));
    let measurement = diagnostic.measurement.expect("both sides are reported");
    assert_eq!(measurement.limit, 1024);
    assert_eq!(measurement.observed, 99_999);
    assert_eq!(
        diagnostic.remediation(),
        ReasonCode::LimitExceeded.remediation()
    );
}

#[test]
fn a_diagnostic_serialises_with_its_stable_codes() {
    let diagnostic = Diagnostic::new(Outcome::Unsupported, ReasonCode::EncryptionPresent);
    let json = serde_json::to_string(&diagnostic).expect("diagnostics are reportable");
    assert!(json.contains("\"unsupported\""));
    assert!(json.contains("\"encryption_present\""));
}

// ── The manifest ────────────────────────────────────────────────────────────

#[test]
fn the_manifest_carries_the_limits_settled_in_the_spec() {
    assert_eq!(LIMITS.max_physical_source_bytes, 128 * 1024 * 1024 * 1024);
    assert_eq!(LIMITS.max_archive_members, 10_000);
    assert_eq!(LIMITS.max_normalized_path_bytes, 1024);
    assert_eq!(LIMITS.max_path_component_bytes, 255);
    assert_eq!(LIMITS.max_decoded_member_bytes, 32 * 1024 * 1024 * 1024);
    assert_eq!(LIMITS.max_decoded_archive_bytes, 128 * 1024 * 1024 * 1024);
    assert_eq!(LIMITS.max_compression_ratio, 10_000);
    assert_eq!(LIMITS.max_descriptor_bytes, 1024 * 1024);
    assert_eq!(LIMITS.max_descriptor_lines, 10_000);
    assert_eq!(LIMITS.max_descriptor_references, 1024);
    assert_eq!(LIMITS.candidate_deadline_seconds, 1800);
    assert_eq!(LIMITS.no_progress_deadline_seconds, 60);
}

#[test]
fn the_ratio_ceiling_ignores_the_first_megabyte() {
    // Below the grace threshold the arithmetic is meaningless — a 10-byte file
    // that expands to 100 KiB is ordinary, not an attack.
    assert!(!LIMITS.ratio_exceeded(10, 100 * 1024));
    // Past it, a 10,000:1 expansion is refused.
    let compressed = 2 * 1024 * 1024;
    assert!(!LIMITS.ratio_exceeded(compressed, compressed * 10_000));
    assert!(LIMITS.ratio_exceeded(compressed, compressed * 10_001));
}

#[test]
fn a_nearly_full_disk_offers_no_temporary_budget_rather_than_an_unlimited_one() {
    // Getting the saturation backwards would turn a full disk into an
    // enormous allowance.
    assert_eq!(LIMITS.temporary_budget(0), 0);
    assert_eq!(LIMITS.temporary_budget(1024), 0);
    let reserve = LIMITS.temporary_free_space_reserve_bytes;
    assert_eq!(LIMITS.temporary_budget(reserve + 4096), 4096);
    assert_eq!(
        LIMITS.temporary_budget(u64::MAX),
        LIMITS.max_temporary_bytes,
        "the budget is still capped by the manifest"
    );
}

#[test]
fn accepted_sets_are_allowlists_not_suggestions() {
    assert!(manifest::accepted(manifest::ZIP_METHODS, "deflate"));
    assert!(manifest::accepted(manifest::ZIP_METHODS, "DEFLATE"));
    // bzip2 is a real ZIP method that this release does not accept. A zip
    // crate gaining support for it must not widen what we import.
    assert!(!manifest::accepted(manifest::ZIP_METHODS, "bzip2"));
    assert!(!manifest::accepted(manifest::CHD_CODECS, "avhu"));
    assert!(manifest::accepted(manifest::CHD_CODECS, "cdlz"));
}

#[test]
fn os_metadata_is_recognised_in_all_the_shapes_it_takes() {
    for name in [
        ".DS_Store",
        "sub/.DS_Store",
        "Thumbs.db",
        "desktop.ini",
        "__MACOSX/game.bin",
        "sub/__MACOSX/game.bin",
        "._game.bin",
        "sub/._game.bin",
    ] {
        assert!(manifest::is_os_metadata(name), "{name} is OS metadata");
    }
    assert!(!manifest::is_os_metadata("game.bin"));
    assert!(!manifest::is_os_metadata("MACOSX/game.bin"));
}

#[test]
fn sidecar_classes_cover_every_documented_extension() {
    for extension in [
        "txt", "nfo", "md", "rtf", "pdf", "png", "jpg", "jpeg", "gif", "webp", "bmp", "sfv", "md5",
        "sha1", "sha256", "json", "xml", "yaml", "yml",
    ] {
        assert!(
            manifest::is_sidecar_extension(extension),
            "{extension} is an ignorable sidecar"
        );
        assert!(
            manifest::is_sidecar_extension(&extension.to_uppercase()),
            "case must not decide whether a member is ignored"
        );
    }
    // An executable or another archive is never a sidecar — it makes
    // membership ambiguous.
    for extension in ["exe", "zip", "7z", "bin", "cue"] {
        assert!(!manifest::is_sidecar_extension(extension), "{extension}");
    }
}

#[test]
fn the_manifest_revision_is_recorded() {
    // Changing an accepted version, method, directive, or limit requires
    // bumping this and re-running the fixture suite.
    assert_eq!(rom_manager::MANIFEST_REVISION, 1);
}
