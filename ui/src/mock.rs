//! Mock fixtures + canned `secureboot-fix` substitutes for `--mock-status` /
//! `--mock-fix`. Lets the UI be driven through every screen without root.

use std::path::PathBuf;

/// Where to look for fixture JSON files. Dev first, then install location.
pub fn resolve_fixture(name: &str) -> Option<PathBuf> {
    let stem = if name.ends_with(".json") {
        name.to_string()
    } else {
        format!("{name}.json")
    };
    let candidates = [
        // Dev: invoked from repo root via `cargo run` (CARGO_MANIFEST_DIR is `ui/`)
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(&stem),
        // Installed
        PathBuf::from("/usr/share/secureboot-watchdog/fixtures").join(&stem),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Canned shell script for `--mock-fix MODE` — emits the same line format the
/// real `secureboot-fix` does, with timed pauses so the progress UI can be
/// observed. `MODE`:
///   * `success` — fast run, all steps OK, exit 0
///   * `slow`    — slow run (3 s per step), all OK, exit 0
///   * `failure` — fast run, halt at step 3 with error, exit 1
pub fn mock_fix_script(mode: &str) -> &'static str {
    match mode {
        "slow" => MOCK_SLOW,
        "failure" => MOCK_FAILURE,
        _ => MOCK_SUCCESS,
    }
}

const MOCK_SUCCESS: &str = r#"
echo "[STEPS] replace-enrollment|cleanup-key"
echo "[STEP] replace-enrollment"
echo "Replacing TPM enrollment (old enrollment retained until new enrollment succeeds)"
sleep 0.4
echo "[STEP] cleanup-key"
sleep 0.4
echo "TPM enrollment replaced. A subsequent boot has not been verified."
exit 0
"#;

const MOCK_SLOW: &str = r#"
echo "[STEPS] replace-enrollment|cleanup-key"
echo "[STEP] replace-enrollment"
echo "Replacing TPM enrollment (old enrollment retained until new enrollment succeeds)"
sleep 2
echo "[STEP] cleanup-key"
sleep 2
echo "TPM enrollment replaced. A subsequent boot has not been verified."
exit 0
"#;

const MOCK_FAILURE: &str = r#"
echo "[STEPS] replace-enrollment|cleanup-key"
echo "[STEP] replace-enrollment"
sleep 0.4
echo "TPM enrollment replacement failed. Review disk enrollment before retrying."
exit 1
"#;
