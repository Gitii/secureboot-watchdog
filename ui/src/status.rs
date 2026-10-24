//! Status file parsing + per-code classification helpers.
//!
//! Single source of truth for what kind of secret each issue code needs, and
//! for the "info-only vs. fixable" distinction. Mirrors the bash logic in
//! `bin/secureboot-notify`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DEFAULT_STATUS_PATH: &str = "/run/secureboot-monitor/status.json";
pub const DEFAULT_MOK_DIR: &str = "/var/lib/shim-signed/mok";

#[derive(Debug, Deserialize, Clone)]
pub struct Status {
    pub ok: bool,
    pub checked_at: String,
    pub kernel: String,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Issue {
    pub code: String,
    pub severity: Severity,
    pub summary: String,
    pub detail: String,
    pub auto_fixable: bool,
    /// Journal provenance, retained separately from human-readable guidance.
    #[serde(default)]
    pub tpm_evidence: Vec<TpmEvidence>,
    #[serde(default)]
    pub tpm_preflight: Option<TpmPreflight>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TpmPreflight {
    pub eligible: bool,
    pub reason: String,
    pub plan: Option<TpmPlan>,
}

/// Confirmation constraints, not authority. Root derives and compares every
/// field, including the complete token metadata, before executing a repair.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TpmPlan {
    pub luks_uuid: String,
    pub mapping: String,
    pub source: String,
    pub crypttab_options: String,
    pub token: TpmToken,
    pub policy: TpmPolicy,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TpmToken {
    pub id: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TpmPolicy {
    pub pcrs: Vec<u8>,
    pub bank: String,
    pub seal_key_handle: String,
    pub pin: bool,
    pub signed_pcr: bool,
    pub pcrlock: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TpmEvidence {
    pub mapping: String,
    pub source: String,
    pub unit: String,
    pub message: String,
    pub comm: String,
    pub exe: String,
}

impl Issue {
    pub fn can_fix(&self) -> bool {
        self.auto_fixable && !is_info_only(&self.code)
    }

    pub fn details_text(&self) -> String {
        let mut text = self.detail.clone();
        for evidence in &self.tpm_evidence {
            text.push_str(&format!(
                "\n\nJournal provenance: {} ({})\n{}\n{} [{}]\n{}",
                evidence.mapping,
                evidence.source,
                evidence.unit,
                evidence.comm,
                evidence.exe,
                evidence.message
            ));
        }
        if let Some(preflight) = &self.tpm_preflight {
            text.push_str(&format!("\n\nTPM preflight: {}", preflight.reason));
            if preflight.eligible {
                if let Some(plan) = &preflight.plan {
                    text.push_str(&format!(
                        "\nTarget: {} ({})\nLUKS UUID: {}\nToken: {}\nStatic PCRs: {} ({})\nSealing key: {}\nPIN: {}; signed PCR: {}; PCRlock: {}",
                        plan.mapping, plan.source, plan.luks_uuid, plan.token.id,
                        plan.policy.pcrs.iter().map(u8::to_string).collect::<Vec<_>>().join("+"),
                        plan.policy.bank, plan.policy.seal_key_handle,
                        plan.policy.pin, plan.policy.signed_pcr, plan.policy.pcrlock,
                    ));
                }
            }
        }
        text
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl Severity {
    pub fn css_class(self) -> &'static str {
        match self {
            Severity::Critical => "severity-critical",
            Severity::Warning => "severity-warning",
            Severity::Info => "severity-info",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Severity::Critical | Severity::Warning => "dialog-warning-symbolic",
            Severity::Info => "dialog-information-symbolic",
        }
    }

    pub fn caption(self) -> &'static str {
        match self {
            Severity::Critical => "Critical",
            Severity::Warning => "Warning",
            Severity::Info => "Info",
        }
    }
}

/// What kind of secret the fix script will need on stdin for this code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    /// No secret needed; close stdin immediately.
    None,
    /// LUKS passphrase (existing, asked once).
    Luks,
    /// MOK enrollment password (new, asked twice for confirmation).
    Mok,
}

/// Codes whose fixer is info-only — the UI shows a details/instructions
/// screen and no Fix button.
pub fn is_info_only(code: &str) -> bool {
    // TPM repairs stay disabled even for a stale status with auto_fixable=true.
    matches!(
        code,
        "SB_DISABLED" | "LOCKDOWN_OFF" | "MOK_PENDING" | "TPM_PCR_DRIFT"
    )
}

/// Whether the on-disk MOK directory contains any `.priv` file. If not, a
/// fresh MOK will be generated by the fix script and we need to collect an
/// enrollment password.
pub fn mok_dir_has_pair(mok_dir: &str) -> bool {
    let Ok(rd) = fs::read_dir(mok_dir) else {
        return false;
    };
    rd.filter_map(Result::ok)
        .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("priv"))
}

pub fn secret_kind(code: &str, mok_dir: &str) -> SecretKind {
    match code {
        "MOK_NOT_ENROLLED" => SecretKind::Mok,
        "TPM_PCR_DRIFT" => SecretKind::Luks,
        "DKMS_UNSIGNED" | "UNSIGNED_MODULE_LOADED" | "SIGNATURE_FAILURES" => {
            if mok_dir_has_pair(mok_dir) {
                SecretKind::None
            } else {
                SecretKind::Mok
            }
        }
        _ => SecretKind::None,
    }
}

/// Read & parse a status file. `path` may be the real `/run/...` location or
/// a `--mock-status` fixture path.
pub fn load_status(path: &Path) -> Result<Status> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// "4 min ago", "just now", "2 h ago" — used for the footer label.
pub fn humanize_checked_at(checked_at: &str) -> String {
    use std::time::SystemTime;
    let Ok(parsed) = parse_rfc3339(checked_at) else {
        return checked_at.to_string();
    };
    let Ok(now) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) else {
        return checked_at.to_string();
    };
    let now_secs = now.as_secs() as i64;
    let delta = now_secs - parsed;
    if delta < 30 {
        "just now".into()
    } else if delta < 3600 {
        let m = (delta + 30) / 60;
        format!("{m} min ago")
    } else if delta < 86_400 {
        let h = delta / 3600;
        format!("{h} h ago")
    } else {
        let d = delta / 86_400;
        format!("{d} d ago")
    }
}

/// Minimal RFC3339 parser — enough for the `"2026-06-17T08:25:31Z"` format
/// our checker emits. Returns Unix epoch seconds.
fn parse_rfc3339(s: &str) -> Result<i64> {
    // Y M D T h m s Z
    let bytes = s.as_bytes();
    anyhow::ensure!(bytes.len() >= 20, "too short");
    let year: i64 = s[0..4].parse()?;
    let month: i64 = s[5..7].parse()?;
    let day: i64 = s[8..10].parse()?;
    let hour: i64 = s[11..13].parse()?;
    let min: i64 = s[14..16].parse()?;
    let sec: i64 = s[17..19].parse()?;
    Ok(days_from_civil(year, month, day) * 86_400 + hour * 3600 + min * 60 + sec)
}

/// Howard Hinnant's date algorithm — days since 1970-01-01 for (y,m,d).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tpm_fixture_retains_and_displays_provenance() {
        let status: Status =
            serde_json::from_str(include_str!("../fixtures/screen4_luks_tpm.json")).unwrap();
        let issue = &status.issues[0];
        assert!(!issue.can_fix());
        let evidence = &issue.tpm_evidence[0];
        assert_eq!(evidence.source, "UUID=22222222-2222-2222-2222-222222222222");
        let text = issue.details_text();
        for value in [
            "4+12",
            "sha384",
            "0x81000001",
            "Token: 4",
            "automatic repair remains disabled",
        ] {
            assert!(text.contains(value), "missing plan detail: {value}");
        }
        let preflight = issue.tpm_preflight.as_ref().unwrap();
        assert!(preflight.eligible);
        let plan = preflight.plan.as_ref().unwrap();
        let roundtrip: TpmPlan =
            serde_json::from_str(&serde_json::to_string(plan).unwrap()).unwrap();
        assert_eq!(plan, &roundtrip);
        let mut stale = issue.clone();
        stale.auto_fixable = true;
        assert!(!stale.can_fix());
        for value in [
            &evidence.mapping,
            &evidence.source,
            &evidence.unit,
            &evidence.message,
            &evidence.comm,
            &evidence.exe,
        ] {
            assert!(text.contains(value));
        }
        let other: Status =
            serde_json::from_str(include_str!("../fixtures/screen2_multi.json")).unwrap();
        assert!(other
            .issues
            .iter()
            .all(|issue| issue.tpm_evidence.is_empty()));
    }

    #[test]
    fn info_only_classification() {
        assert!(is_info_only("SB_DISABLED"));
        assert!(is_info_only("LOCKDOWN_OFF"));
        assert!(is_info_only("MOK_PENDING"));
        assert!(!is_info_only("DKMS_UNSIGNED"));
        assert!(is_info_only("TPM_PCR_DRIFT"));
    }

    #[test]
    fn secret_kind_for_codes() {
        assert_eq!(
            secret_kind("MOK_NOT_ENROLLED", "/nonexistent"),
            SecretKind::Mok
        );
        assert_eq!(
            secret_kind("TPM_PCR_DRIFT", "/nonexistent"),
            SecretKind::Luks
        );
        assert_eq!(
            secret_kind("EFI_UNSIGNED", "/nonexistent"),
            SecretKind::None
        );
        // DKMS_UNSIGNED with no MOK pair -> generate fresh -> needs password
        assert_eq!(
            secret_kind("DKMS_UNSIGNED", "/nonexistent"),
            SecretKind::Mok
        );
    }

    #[test]
    fn humanize_relative() {
        // Smoke: doesn't panic, produces a non-empty string.
        let s = humanize_checked_at("2026-06-17T08:25:31Z");
        assert!(!s.is_empty());
    }
}
