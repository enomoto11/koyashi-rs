//! End-to-end tests that run the `koyashi` binary against fixture crates.
//!
//! These exercise the full pipeline — `cargo metadata`, the rust-analyzer LSP
//! session, and reference classification — which the unit tests cannot reach.
//! Tests that need rust-analyzer are skipped (not failed) when it is missing,
//! so a contributor without it installed can still run `cargo test`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Absolute path to a fixture crate under `tests/fixtures/`.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Whether a rust-analyzer executable can be located, mirroring the binary's
/// own resolution: `KOYASHI_RUST_ANALYZER` first, then `PATH`.
fn rust_analyzer_available() -> bool {
    if let Some(explicit) = std::env::var_os("KOYASHI_RUST_ANALYZER") {
        return Path::new(&explicit).is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths)
        .any(|dir| dir.join("rust-analyzer").is_file() || dir.join("rust-analyzer.exe").is_file())
}

/// Run `koyashi check` with the given arguments, returning the process output.
fn run_check(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_koyashi"))
        .arg("check")
        .args(args)
        .output()
        .expect("failed to spawn the koyashi binary")
}

/// Skip the enclosing test unless rust-analyzer is installed.
macro_rules! require_rust_analyzer {
    () => {
        if !rust_analyzer_available() {
            eprintln!("skipping: rust-analyzer not found on PATH");
            return;
        }
    };
}

#[test]
fn freeloaders_text_reports_every_classification() {
    require_rust_analyzer!();
    let output = run_check(&["--workspace", fixture("freeloaders").to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert_eq!(
        output.status.code(),
        Some(1),
        "a finding above the threshold should set exit code 1; stderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(stdout.contains("read-only") && stdout.contains("Settings::label"));
    assert!(stdout.contains("write-only") && stdout.contains("Settings::last_message"));
    assert!(stdout.contains("unused") && stdout.contains("Orphan::forgotten"));
    assert!(stdout.contains("derive-only") && stdout.contains("Telemetry::trace_id"));

    // Healthy fields — read and written, or reached only by a mutable borrow —
    // must not be reported.
    assert!(!stdout.contains("Settings::counter"));
    assert!(!stdout.contains("Budget::remaining"));

    // The write-only finding names the line at which the write occurs.
    assert!(stdout.contains("0 reads, 1 writes (lines 49)"));
}

#[test]
fn freeloaders_json_summary_counts_each_class() {
    require_rust_analyzer!();
    let output = run_check(&[
        "--workspace",
        fixture("freeloaders").to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(1));

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");

    assert_eq!(report["schema_version"], "0.1");
    let summary = &report["summary"];
    assert_eq!(summary["unused"], 1);
    assert_eq!(summary["write_only"], 1);
    assert_eq!(summary["read_only"], 1);
    assert_eq!(summary["derive_only"], 1);
    assert_eq!(report["findings"].as_array().map(Vec::len), Some(4));
}

#[test]
fn clean_fixture_reports_nothing() {
    require_rust_analyzer!();
    let output = run_check(&["--workspace", fixture("clean").to_str().unwrap()]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "a clean crate should set exit code 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "no koyashi fields found",
    );
}

#[test]
fn struct_filter_restricts_output_to_one_struct() {
    require_rust_analyzer!();
    let output = run_check(&[
        "--workspace",
        fixture("freeloaders").to_str().unwrap(),
        "--struct",
        "Settings",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Settings::label"));
    assert!(stdout.contains("Settings::last_message"));
    assert!(!stdout.contains("Orphan::forgotten"));
    assert!(!stdout.contains("Telemetry::trace_id"));
}

#[test]
fn explain_lists_reference_sites() {
    require_rust_analyzer!();
    let output = run_check(&[
        "--workspace",
        fixture("freeloaders").to_str().unwrap(),
        "--struct",
        "Settings",
        "--explain",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("reference site(s):"));
    // `last_message` is initialized once and written once.
    assert!(stdout.contains("initializer"));
    assert!(stdout.contains("write"));
}

#[test]
fn missing_rust_analyzer_yields_exit_code_two() {
    // Override resolution with a path that does not exist: workspace resolution
    // still succeeds, but starting the analyzer fails, which is a runtime error.
    let output = Command::new(env!("CARGO_BIN_EXE_koyashi"))
        .arg("check")
        .arg("--workspace")
        .arg(fixture("clean"))
        .env("KOYASHI_RUST_ANALYZER", "/no/such/rust-analyzer")
        .output()
        .expect("failed to spawn the koyashi binary");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("non-existent file"),
        "the error should explain that the override path is missing",
    );
}
