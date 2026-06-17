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
echo "[STEPS] clear-lockout|wipe-old-slot|seal-new-slot|finalize"
echo "[STEP] clear-lockout"
echo "+ tpm2_dictionarylockout --clear-lockout"
sleep 0.4
echo "[STEP] wipe-old-slot"
echo "+ systemd-cryptenroll --wipe-slot=tpm2 /dev/nvme0n1p3"
sleep 0.4
echo "[STEP] seal-new-slot"
echo "+ systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7+11 /dev/nvme0n1p3"
sleep 0.4
echo "[STEP] finalize"
echo "+ systemctl start --no-block secureboot-monitor.service"
exit 0
"#;

const MOCK_SLOW: &str = r#"
echo "[STEPS] clear-lockout|wipe-old-slot|seal-new-slot|finalize"
echo "[STEP] clear-lockout"
echo "+ tpm2_dictionarylockout --clear-lockout"
sleep 2
echo "[STEP] wipe-old-slot"
echo "+ systemd-cryptenroll --wipe-slot=tpm2 /dev/nvme0n1p3"
sleep 2
echo "[STEP] seal-new-slot"
echo "+ systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7+11 /dev/nvme0n1p3"
sleep 2
echo "[STEP] finalize"
echo "+ systemctl start --no-block secureboot-monitor.service"
exit 0
"#;

const MOCK_FAILURE: &str = r#"
echo "[STEPS] clear-lockout|wipe-old-slot|seal-new-slot|finalize"
echo "[STEP] clear-lockout"
echo "+ tpm2_dictionarylockout --clear-lockout"
sleep 0.4
echo "[STEP] wipe-old-slot"
echo "+ systemd-cryptenroll --wipe-slot=tpm2 /dev/nvme0n1p3"
sleep 0.4
echo "[STEP] seal-new-slot"
echo "+ systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7+11 /dev/nvme0n1p3"
echo "Failed to setup TPM2 keyslot: No such file or directory"
exit 1
"#;
