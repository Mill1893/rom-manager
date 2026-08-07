//! The fixture-seeded filesystem tracer (issues #35, #37, #77).
//!
//! # Why this exists as a program
//!
//! The automated suite proves the sync core against temporary directories on
//! whatever host CI happens to run. That is not the same claim as "this works
//! on the SD card in your handheld", and #37 and #77 both ask for the second
//! one. A card reader presents a real FAT32 or exFAT volume with real capacity,
//! real case-insensitivity, and real removal semantics, and none of those are
//! reproducible from a test fixture.
//!
//! So the scenarios live here as a program a person can point at actual media:
//!
//! ```text
//! rom-manager-tracer --target E:\
//! rom-manager-tracer --target /media/andy/SDCARD --json
//! ```
//!
//! # It confines itself, and it cleans up
//!
//! The tracer works inside one directory it creates beneath the target and
//! removes when it finishes. Pointing it at a card holding real ROMs must not
//! put them at risk — a validation tool that can eat the thing it validates is
//! worse than no tool, because it will be run on exactly the media the user
//! cares about.
//!
//! # Honest about what it did not do
//!
//! Some scenarios cannot be reproduced against arbitrary media. Filling a
//! 256 GB card to prove capacity blocking is not something a validation run
//! should do to someone's hardware. Those are reported as `skipped` with the
//! reason, never quietly counted as passes — the whole point of this program is
//! evidence, and a pass that did not happen is worse than a gap that is named.

use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use rom_manager::{
    Action, Approval, CancellationToken, DeviceProfile, ExecutionOutcome, FilesystemTransport,
    ManagedArtifactManifest, ManagedEvidence, ManagementOrigin, RelativePath, Store, SyncCore,
    TargetArtifact, TransportCapabilities, sha256,
};

/// The fixture ROM Set. Project-generated, not a dump — see `fixtures/nes`.
const ROM_BYTES: &[u8] = include_bytes!("../../fixtures/nes/tracers.nes");
const ROM_SET_ID: &str = "rom-set-tracer";
const TARGET_ID: &str = "tracer-target-001";
const DESIRED: &str = "ROMs/nes/Tracers.nes";
/// The directory the tracer creates and removes. Named so a human who finds it
/// after a crash knows what it is and that deleting it is safe.
const WORKSPACE: &str = "rom-manager-tracer-workspace";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Passed,
    Failed,
    Skipped,
}

impl Verdict {
    fn label(&self) -> &'static str {
        match self {
            Self::Passed => "pass",
            Self::Failed => "FAIL",
            Self::Skipped => "skip",
        }
    }
}

struct Outcome {
    scenario: &'static str,
    verdict: Verdict,
    detail: String,
}

fn passed(scenario: &'static str, detail: impl Into<String>) -> Outcome {
    Outcome {
        scenario,
        verdict: Verdict::Passed,
        detail: detail.into(),
    }
}

fn failed(scenario: &'static str, detail: impl Into<String>) -> Outcome {
    Outcome {
        scenario,
        verdict: Verdict::Failed,
        detail: detail.into(),
    }
}

fn skipped(scenario: &'static str, detail: impl Into<String>) -> Outcome {
    Outcome {
        scenario,
        verdict: Verdict::Skipped,
        detail: detail.into(),
    }
}

fn path(value: &str) -> RelativePath {
    RelativePath::new(value).expect("the fixture path is a valid target path")
}

fn expected() -> TargetArtifact {
    TargetArtifact::new(ROM_SET_ID, path(DESIRED), ROM_BYTES.to_vec())
}

/// A core wanting exactly the fixture ROM Set on a fresh directory.
fn core_on(root: &Path) -> Result<SyncCore<FilesystemTransport>, String> {
    let transport = FilesystemTransport::new(root).map_err(|error| error.to_string())?;
    Ok(SyncCore::new(
        transport,
        TARGET_ID,
        DeviceProfile::generic_nes(),
        vec![expected()],
        1,
    ))
}

/// Plans and executes once, returning the outcome.
fn sync_once(core: &mut SyncCore<FilesystemTransport>) -> Result<ExecutionOutcome, String> {
    core.refresh().map_err(|error| format!("{error:?}"))?;
    let plan = core.build_plan().map_err(|error| format!("{error:?}"))?;
    let removals = plan
        .actions
        .iter()
        .filter(|action| action.action == Action::Remove)
        .count();
    let approval = Approval::grant(&plan, removals);
    core.execute(&plan, approval, &CancellationToken::default())
        .map_err(|error| format!("{error:?}"))
}

// ── Scenarios ───────────────────────────────────────────────────────────────

fn marker_initialization(root: &Path) -> Outcome {
    const NAME: &str = "marker initialization";
    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if let Err(error) = core.initialize_target(true) {
        return failed(
            NAME,
            format!("a fresh target did not initialize: {error:?}"),
        );
    }
    // The marker is what makes this directory a Media Target rather than a
    // folder the application happens to write to.
    let marker = root.join("ROMManager").join("target.json");
    if !marker.exists() {
        return failed(NAME, format!("no marker at {}", marker.display()));
    }
    match fs::read_to_string(&marker) {
        Ok(text) if text.contains(TARGET_ID) => {
            passed(NAME, format!("marker written, {} bytes", text.len()))
        }
        Ok(_) => failed(NAME, "the marker does not name this target"),
        Err(error) => failed(NAME, format!("the marker is unreadable: {error}")),
    }
}

fn add_and_read_back(root: &Path) -> Outcome {
    const NAME: &str = "add and read-back verification";
    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if let Err(error) = core.initialize_target(true) {
        return failed(NAME, format!("{error:?}"));
    }
    match sync_once(&mut core) {
        Ok(ExecutionOutcome::Completed { report }) => {
            let placed = root.join("ROMs").join("nes").join("Tracers.nes");
            match fs::read(&placed) {
                Ok(bytes) if sha256(&bytes) == sha256(ROM_BYTES) => {
                    passed(NAME, format!("{} bytes placed and verified", bytes.len()))
                }
                Ok(bytes) => failed(
                    NAME,
                    format!(
                        "the file on the target is {} bytes and does not match",
                        bytes.len()
                    ),
                ),
                Err(error) => failed(NAME, format!("nothing at {}: {error}", placed.display())),
            }
            .with_report(report.performed.len())
        }
        Ok(other) => failed(NAME, format!("expected completion, got {other:?}")),
        Err(error) => failed(NAME, error),
    }
}

impl Outcome {
    fn with_report(mut self, performed: usize) -> Self {
        if self.verdict == Verdict::Passed {
            self.detail = format!("{}, {performed} action(s) performed", self.detail);
        }
        self
    }
}

fn retain_on_second_run(root: &Path) -> Outcome {
    const NAME: &str = "retain";
    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if core.initialize_target(true).is_err() {
        return failed(NAME, "initialization failed");
    }
    if let Err(error) = sync_once(&mut core) {
        return failed(NAME, format!("the first sync failed: {error}"));
    }

    // The second run must want nothing. A tool that rewrites identical content
    // every time is one that wears out flash and cannot be trusted to say what
    // it changed.
    if let Err(error) = core.refresh() {
        return failed(NAME, format!("{error:?}"));
    }
    match core.build_plan() {
        Ok(plan) => {
            let mutating = plan
                .actions
                .iter()
                .filter(|action| action.action != Action::Retain)
                .count();
            if mutating == 0 {
                passed(NAME, "the second run wants no changes")
            } else {
                failed(NAME, format!("{mutating} action(s) on an unchanged target"))
            }
        }
        Err(error) => failed(NAME, format!("{error:?}")),
    }
}

fn changed_locator(root: &Path, moved: &Path) -> Outcome {
    const NAME: &str = "changed locator";
    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if core.initialize_target(true).is_err() {
        return failed(NAME, "initialization failed");
    }
    if let Err(error) = sync_once(&mut core) {
        return failed(NAME, format!("the first sync failed: {error}"));
    }
    drop(core);

    // A card that comes back as a different drive letter is the same Media
    // Target. Identity lives in the marker, not the path.
    if let Err(error) = fs::rename(root, moved) {
        return skipped(NAME, format!("the target could not be relocated: {error}"));
    }
    let mut relocated = match core_on(moved) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    match relocated.refresh() {
        Ok(()) => match relocated.build_plan() {
            Ok(plan)
                if plan
                    .actions
                    .iter()
                    .all(|action| action.action == Action::Retain) =>
            {
                passed(NAME, "the relocated target kept its identity and manifest")
            }
            Ok(_) => failed(NAME, "the relocated target wanted to rewrite its contents"),
            Err(error) => failed(NAME, format!("{error:?}")),
        },
        Err(error) => failed(NAME, format!("{error:?}")),
    }
}

fn adoption(root: &Path) -> Outcome {
    const NAME: &str = "adoption";
    // Content already at the desired path, byte-identical, placed by something
    // else. It should be adopted rather than rewritten.
    let desired = root.join("ROMs").join("nes");
    if fs::create_dir_all(&desired).is_err() {
        return failed(NAME, "could not seed the target");
    }
    if fs::write(desired.join("Tracers.nes"), ROM_BYTES).is_err() {
        return failed(NAME, "could not seed the artifact");
    }

    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if core.initialize_target(true).is_err() {
        return failed(NAME, "initialization failed");
    }
    match sync_once(&mut core) {
        Ok(ExecutionOutcome::Completed { .. }) => match fs::read(desired.join("Tracers.nes")) {
            Ok(bytes) if bytes == ROM_BYTES => {
                passed(NAME, "pre-existing identical content was adopted intact")
            }
            Ok(_) => failed(NAME, "the adopted content changed"),
            Err(error) => failed(NAME, format!("{error}")),
        },
        Ok(other) => failed(NAME, format!("{other:?}")),
        Err(error) => failed(NAME, error),
    }
}

fn conflict(root: &Path) -> Outcome {
    const NAME: &str = "conflict";
    // Different content at the desired path that the application did not place.
    let desired = root.join("ROMs").join("nes");
    if fs::create_dir_all(&desired).is_err() {
        return failed(NAME, "could not seed the target");
    }
    if fs::write(desired.join("Tracers.nes"), b"not the fixture").is_err() {
        return failed(NAME, "could not seed the conflict");
    }

    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if core.initialize_target(true).is_err() {
        return failed(NAME, "initialization failed");
    }
    if let Err(error) = core.refresh() {
        return failed(NAME, format!("{error:?}"));
    }
    let plan = match core.build_plan() {
        Ok(plan) => plan,
        // Refusing to plan at all is the strongest form of not overwriting.
        Err(error) => return passed(NAME, format!("planning refused: {error:?}")),
    };

    if !plan.is_executable() {
        return passed(NAME, format!("the plan blocked: {:?}", plan.blocked));
    }

    // An executable plan is fine as long as the invariant holds: bytes the
    // application did not place are still there when it finishes.
    let outcome = core.execute(
        &plan,
        Approval::grant(&plan, 0),
        &CancellationToken::default(),
    );
    match fs::read(desired.join("Tracers.nes")) {
        Ok(bytes) if bytes == b"not the fixture" => {
            passed(NAME, "unmanaged content was left untouched")
        }
        Ok(_) => failed(
            NAME,
            format!("unmanaged content was overwritten (outcome {outcome:?})"),
        ),
        Err(error) => failed(NAME, format!("unmanaged content vanished: {error}")),
    }
}

fn managed_removal(root: &Path) -> Outcome {
    const NAME: &str = "managed removal";
    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if core.initialize_target(true).is_err() {
        return failed(NAME, "initialization failed");
    }
    if let Err(error) = sync_once(&mut core) {
        return failed(NAME, format!("the first sync failed: {error}"));
    }

    // Now want nothing. What the application placed, it may remove — with an
    // acknowledgement, never implicitly.
    let mut emptied = SyncCore::new(
        match FilesystemTransport::new(root) {
            Ok(transport) => transport,
            Err(error) => return failed(NAME, error.to_string()),
        },
        TARGET_ID,
        DeviceProfile::generic_nes(),
        Vec::new(),
        1,
    );
    emptied.replace_local_manifest(core.local_manifest().cloned());
    match sync_once(&mut emptied) {
        Ok(ExecutionOutcome::Completed { .. }) => {
            let placed = root.join("ROMs").join("nes").join("Tracers.nes");
            if placed.exists() {
                failed(NAME, "the managed artifact survived a removal plan")
            } else {
                passed(NAME, "the managed artifact was removed")
            }
        }
        Ok(other) => failed(NAME, format!("{other:?}")),
        Err(error) => failed(NAME, error),
    }
}

fn cancellation(root: &Path) -> Outcome {
    const NAME: &str = "cancellation";
    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if core.initialize_target(true).is_err() {
        return failed(NAME, "initialization failed");
    }
    if let Err(error) = core.refresh() {
        return failed(NAME, format!("{error:?}"));
    }
    let plan = match core.build_plan() {
        Ok(plan) => plan,
        Err(error) => return failed(NAME, format!("{error:?}")),
    };
    let approval = Approval::grant(&plan, 0);

    // Cancelled before anything starts. The outcome must say so, and must not
    // claim completion.
    let token = CancellationToken::default();
    token.cancel();
    match core.execute(&plan, approval, &token) {
        Ok(ExecutionOutcome::Cancelled { report }) => passed(
            NAME,
            format!(
                "cancelled cleanly, {} action(s) not attempted",
                report.not_attempted.len()
            ),
        ),
        Ok(other) => failed(NAME, format!("expected cancellation, got {other:?}")),
        Err(error) => failed(NAME, format!("{error:?}")),
    }
}

fn mutation_between_planning_and_execution(root: &Path) -> Outcome {
    const NAME: &str = "post-plan mutation";
    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if core.initialize_target(true).is_err() {
        return failed(NAME, "initialization failed");
    }
    if let Err(error) = core.refresh() {
        return failed(NAME, format!("{error:?}"));
    }
    let plan = match core.build_plan() {
        Ok(plan) => plan,
        Err(error) => return failed(NAME, format!("{error:?}")),
    };
    let approval = Approval::grant(&plan, 0);

    // Someone writes to the card between the user approving and the write
    // starting. The approval described a target that no longer exists.
    let intruder = root.join("ROMs").join("nes");
    let _ = fs::create_dir_all(&intruder);
    if fs::write(intruder.join("Someone-Else.nes"), b"added behind our back").is_err() {
        return skipped(NAME, "the target could not be mutated for this check");
    }

    match core.execute(&plan, approval, &CancellationToken::default()) {
        Err(error) => passed(NAME, format!("the stale approval was refused: {error:?}")),
        Ok(outcome) => failed(
            NAME,
            format!("a plan built against different target state executed: {outcome:?}"),
        ),
    }
}

fn manifest_agreement(root: &Path) -> Outcome {
    const NAME: &str = "manifest agreement";
    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if core.initialize_target(true).is_err() {
        return failed(NAME, "initialization failed");
    }
    if let Err(error) = sync_once(&mut core) {
        return failed(NAME, format!("the first sync failed: {error}"));
    }

    // A manifest claiming content that is not what is actually there must block
    // destructive authority rather than be believed.
    let mut lying = ManagedArtifactManifest::empty(TARGET_ID, &DeviceProfile::generic_nes());
    lying.generation = 99;
    lying.artifacts.insert(
        path(DESIRED),
        ManagedEvidence {
            rom_set_id: ROM_SET_ID.into(),
            size: 1,
            sha256: sha256(b"a different file entirely"),
            origin: ManagementOrigin::Placed,
        },
    );
    core.replace_local_manifest(Some(lying));

    if let Err(error) = core.refresh() {
        return passed(
            NAME,
            format!("a disagreeing manifest blocked refresh: {error:?}"),
        );
    }
    match core.build_plan() {
        Err(error) => passed(
            NAME,
            format!("a disagreeing manifest blocked planning: {error:?}"),
        ),
        Ok(plan) if !plan.is_executable() => {
            passed(NAME, "a disagreeing manifest produced a blocked plan")
        }
        Ok(_) => failed(NAME, "a manifest disagreeing with the target was believed"),
    }
}

fn refresh_and_new_plan_recovery(root: &Path) -> Outcome {
    const NAME: &str = "refresh plus new-plan recovery";
    let mut core = match core_on(root) {
        Ok(core) => core,
        Err(error) => return failed(NAME, error),
    };
    if core.initialize_target(true).is_err() {
        return failed(NAME, "initialization failed");
    }
    if let Err(error) = core.refresh() {
        return failed(NAME, format!("{error:?}"));
    }
    let stale = match core.build_plan() {
        Ok(plan) => plan,
        Err(error) => return failed(NAME, format!("{error:?}")),
    };

    // Change the target, so the first plan is stale.
    let seeded = root.join("ROMs").join("nes");
    let _ = fs::create_dir_all(&seeded);
    let _ = fs::write(seeded.join("Unrelated.nes"), b"changed underneath");

    if core
        .execute(
            &stale,
            Approval::grant(&stale, 0),
            &CancellationToken::default(),
        )
        .is_ok()
    {
        return failed(NAME, "a stale plan executed");
    }

    // The documented recovery: refresh, build a new plan, and proceed.
    if let Err(error) = core.refresh() {
        return failed(NAME, format!("refresh after staleness failed: {error:?}"));
    }
    match core.build_plan() {
        Ok(fresh) => {
            let approval = Approval::grant(&fresh, 0);
            match core.execute(&fresh, approval, &CancellationToken::default()) {
                Ok(ExecutionOutcome::Completed { .. }) => {
                    passed(NAME, "refresh produced a plan that executed")
                }
                Ok(other) => failed(NAME, format!("the recovered plan gave {other:?}")),
                Err(error) => failed(NAME, format!("the recovered plan failed: {error:?}")),
            }
        }
        Err(error) => failed(NAME, format!("no plan after refresh: {error:?}")),
    }
}

fn durable_state_survives_restart(root: &Path) -> Outcome {
    const NAME: &str = "durable state across restart";
    let database = root.join("tracer-state.sqlite3");
    {
        let store = match Store::open(&database) {
            Ok(store) => store,
            Err(error) => return failed(NAME, format!("{error:?}")),
        };
        if store.upsert_target(TARGET_ID, 1).is_err() {
            return failed(NAME, "the target could not be recorded");
        }
        if store
            .record_binding(
                TARGET_ID,
                &root.to_string_lossy(),
                &TransportCapabilities::filesystem(),
                1,
            )
            .is_err()
        {
            return failed(NAME, "the binding could not be recorded");
        }
    }

    // Reopening is the restart. Everything above must still be there, and the
    // schema must not have been reapplied.
    match Store::open(&database) {
        Ok(store) => match store.bindings_for(TARGET_ID) {
            Ok(bindings) if bindings.len() == 1 => passed(
                NAME,
                format!(
                    "schema {} reopened with its bindings intact",
                    store.schema_version().unwrap_or(0)
                ),
            ),
            Ok(bindings) => failed(NAME, format!("{} binding(s) after restart", bindings.len())),
            Err(error) => failed(NAME, format!("{error:?}")),
        },
        Err(error) => failed(NAME, format!("the store did not reopen: {error:?}")),
    }
}

// ── Driver ──────────────────────────────────────────────────────────────────

/// Runs `scenario` in its own freshly created directory.
///
/// Each scenario gets a clean target, because a scenario inheriting the last
/// one's state proves something nobody chose to test.
fn in_fresh_target<F>(workspace: &Path, name: &str, scenario: F) -> Outcome
where
    F: FnOnce(&Path) -> Outcome,
{
    let root = workspace.join(name);
    if let Err(error) = fs::create_dir_all(&root) {
        return failed(
            "workspace",
            format!("could not create {}: {error}", root.display()),
        );
    }
    scenario(&root)
}

struct Options {
    target: PathBuf,
    json: bool,
}

fn parse_arguments() -> Result<Options, String> {
    let mut target = None;
    let mut json = false;
    let mut arguments = std::env::args().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--target" | "-t" => {
                target = Some(PathBuf::from(
                    arguments.next().ok_or("--target needs a path")?,
                ));
            }
            "--json" => json = true,
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unrecognized argument {other}\n\n{}", usage())),
        }
    }
    Ok(Options {
        target: target.ok_or_else(|| format!("--target is required\n\n{}", usage()))?,
        json,
    })
}

fn usage() -> String {
    "\
rom-manager-tracer — the fixture-seeded filesystem tracer

USAGE:
    rom-manager-tracer --target <PATH> [--json]

    --target, -t   The Media Target to validate. A card reader mount, a USB
                   drive, or any directory. The tracer works inside one
                   subdirectory it creates and removes.
    --json         Emit machine-readable results instead of a table.

The tracer never writes outside the subdirectory it creates, and removes it on
the way out."
        .to_owned()
}

fn main() -> ExitCode {
    let options = match parse_arguments() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    if !options.target.is_dir() {
        eprintln!(
            "the target {} is not a directory this process can see",
            options.target.display()
        );
        return ExitCode::from(2);
    }

    let workspace = options.target.join(WORKSPACE);
    // A leftover workspace from an interrupted run is the tracer's own, and
    // removing it is safe. Nothing else is ever touched.
    let _ = fs::remove_dir_all(&workspace);
    if let Err(error) = fs::create_dir_all(&workspace) {
        eprintln!(
            "could not create the workspace at {}: {error}",
            workspace.display()
        );
        return ExitCode::from(2);
    }

    let started = Instant::now();
    let mut outcomes = vec![
        in_fresh_target(&workspace, "marker", marker_initialization),
        in_fresh_target(&workspace, "add", add_and_read_back),
        in_fresh_target(&workspace, "retain", retain_on_second_run),
        in_fresh_target(&workspace, "adoption", adoption),
        in_fresh_target(&workspace, "conflict", conflict),
        in_fresh_target(&workspace, "removal", managed_removal),
        in_fresh_target(&workspace, "cancellation", cancellation),
        in_fresh_target(
            &workspace,
            "mutation",
            mutation_between_planning_and_execution,
        ),
        in_fresh_target(&workspace, "manifest", manifest_agreement),
        in_fresh_target(&workspace, "recovery", refresh_and_new_plan_recovery),
        in_fresh_target(&workspace, "durable", durable_state_survives_restart),
    ];

    // The relocation scenario needs two paths, so it is driven directly.
    let from = workspace.join("locator-from");
    let to = workspace.join("locator-to");
    let _ = fs::create_dir_all(&from);
    outcomes.push(changed_locator(&from, &to));

    // Capacity blocking and physical disconnect are deliberately not attempted.
    // Filling a card to prove the first, or asking a program to unplug itself
    // for the second, are not things a validation run should do to someone's
    // hardware. Both are covered by the automated suite, and the runbooks ask
    // the operator to perform them by hand.
    outcomes.push(skipped(
        "capacity blocking",
        "not attempted: proving it means filling the target. Covered by the \
         automated suite; the runbook asks the operator to use a small volume.",
    ));
    outcomes.push(skipped(
        "disconnect",
        "not attempted: requires physically removing the media mid-write. \
         Covered by the automated suite; the runbook asks the operator to do it.",
    ));

    let elapsed = started.elapsed();
    let _ = fs::remove_dir_all(&workspace);

    let failures = outcomes
        .iter()
        .filter(|outcome| outcome.verdict == Verdict::Failed)
        .count();
    let skips = outcomes
        .iter()
        .filter(|outcome| outcome.verdict == Verdict::Skipped)
        .count();

    if options.json {
        println!("{{");
        println!("  \"target\": {:?},", options.target.display().to_string());
        println!("  \"elapsed_ms\": {},", elapsed.as_millis());
        println!("  \"scenarios\": [");
        for (index, outcome) in outcomes.iter().enumerate() {
            let comma = if index + 1 == outcomes.len() { "" } else { "," };
            println!(
                "    {{\"scenario\": {:?}, \"verdict\": {:?}, \"detail\": {:?}}}{comma}",
                outcome.scenario,
                outcome.verdict.label(),
                outcome.detail
            );
        }
        println!("  ],");
        println!("  \"failed\": {failures},");
        println!("  \"skipped\": {skips}");
        println!("}}");
    } else {
        println!("ROM Manager filesystem tracer");
        println!("target: {}", options.target.display());
        println!();
        for outcome in &outcomes {
            println!("  [{}] {}", outcome.verdict.label(), outcome.scenario);
            println!("         {}", outcome.detail);
        }
        println!();
        println!(
            "{} scenario(s): {} passed, {failures} failed, {skips} skipped, in {:.1}s",
            outcomes.len(),
            outcomes.len() - failures - skips,
            elapsed.as_secs_f64()
        );
    }

    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
