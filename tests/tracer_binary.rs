//! The tracer binary's own contract (issues #35, #37, #77).
//!
//! The scenarios it runs are covered by the suites they came from. What is
//! covered *here* is the property those suites cannot see: this program gets
//! pointed at media holding someone's real ROM collection, so it must confine
//! itself to the directory it creates and leave nothing behind.

use std::{fs, path::Path, process::Command};

/// The tracer as cargo built it, beside this test binary.
fn tracer() -> std::path::PathBuf {
    let mut path = std::env::current_exe().expect("the test binary has a path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join(format!(
        "rom-manager-tracer{}",
        std::env::consts::EXE_SUFFIX
    ))
}

fn run(target: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(tracer())
        .arg("--target")
        .arg(target)
        .args(extra)
        .output()
        .expect("the tracer runs")
}

#[test]
fn every_scenario_passes_against_an_ordinary_directory() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &[]);
    let text = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "the tracer reported a failure:\n{text}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!text.contains("[FAIL]"), "a scenario failed:\n{text}");
}

#[test]
fn nothing_the_tracer_created_is_left_behind() {
    // The property that matters when this is pointed at a card holding real
    // ROMs. A validation tool that leaves debris on the media it validates
    // will be trusted less than one that does nothing.
    let directory = tempfile::tempdir().unwrap();

    // A file that was there first, and must still be there after.
    let bystander = directory.path().join("someones-game.nes");
    fs::write(&bystander, b"content the tracer must not touch").unwrap();

    assert!(run(directory.path(), &[]).status.success());

    let remaining: Vec<String> = fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();

    assert_eq!(
        remaining,
        vec!["someones-game.nes"],
        "the tracer left something behind: {remaining:?}"
    );
    assert_eq!(
        fs::read(&bystander).unwrap(),
        b"content the tracer must not touch",
        "the tracer modified a file it did not create"
    );
}

#[test]
fn a_leftover_workspace_from_an_interrupted_run_is_reclaimed() {
    // Its own debris is safe to remove; nothing else ever is.
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("rom-manager-tracer-workspace");
    fs::create_dir_all(workspace.join("half-finished")).unwrap();
    fs::write(workspace.join("stale.txt"), b"from a run that died").unwrap();

    assert!(run(directory.path(), &[]).status.success());
    assert!(!workspace.exists(), "the stale workspace survived");
}

#[test]
fn the_json_report_names_every_scenario_and_its_verdict() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--json"]);
    let text = String::from_utf8_lossy(&output.stdout);

    let report: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("not JSON: {error}\n{text}"));

    let scenarios = report["scenarios"].as_array().expect("a scenario array");
    assert!(scenarios.len() >= 14, "only {} scenarios", scenarios.len());
    assert_eq!(report["failed"], 0);

    for scenario in scenarios {
        let verdict = scenario["verdict"].as_str().expect("a verdict");
        assert!(
            ["pass", "FAIL", "skip"].contains(&verdict),
            "unexpected verdict {verdict}"
        );
        assert!(
            !scenario["detail"].as_str().unwrap_or_default().is_empty(),
            "every verdict carries its reasoning: {scenario}"
        );
    }
}

#[test]
fn the_scenarios_that_were_not_attempted_say_so_rather_than_passing() {
    // A pass that did not happen is worse than a gap that is named.
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), &["--json"]);
    let report: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).expect("the report parses");

    let skipped: Vec<&str> = report["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|scenario| scenario["verdict"] == "skip")
        .map(|scenario| scenario["scenario"].as_str().unwrap())
        .collect();

    assert!(skipped.contains(&"capacity blocking"), "{skipped:?}");
    assert!(skipped.contains(&"disconnect"), "{skipped:?}");
    assert_eq!(report["skipped"], skipped.len());
}

#[test]
fn a_missing_target_is_refused_rather_than_created() {
    // Creating it would mean a typo silently validates an empty directory on
    // the wrong volume and reports a clean pass.
    let directory = tempfile::tempdir().unwrap();
    let absent = directory.path().join("no-such-drive");

    let output = run(&absent, &[]);
    assert!(!output.status.success());
    assert!(!absent.exists(), "the tracer created its own target");
}

#[test]
fn the_target_argument_is_required() {
    let output = Command::new(tracer()).output().expect("the tracer runs");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--target"),
        "the error should name the missing argument"
    );
}
