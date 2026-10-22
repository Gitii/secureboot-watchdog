# Bound TPM repair to reproducible existing enrollment

Status: accepted 2026-10-22

## The Problem

Repair must not guess the affected disk, silently change its TPM policy, or destroy working enrollment before replacement succeeds.

## Options Considered

- Keep first-disk selection, default PCRs 7+11, preemptive slot deletion, and automatic lockout reset.
- **Repair only an unambiguous affected disk with a supported, reproducible existing policy; otherwise require manual recovery.**
- Build a disk and enrollment-policy selection UI.

## Rationale

Conservative repair keeps the existing workflow without expanding it into a policy-management tool. Re-enrollment is not permission to change security policy or reset TPM-wide dictionary-attack state.

## Notes

- Revalidate disk identity and enrollment as root at repair time, rather than trusting UI data or choosing the first crypttab entry.
- Preserve existing policy parameters; never substitute PCRs 7+11 or an environment override.
- Unsupported or ambiguous enrollment gets guidance and no enrollment changes.
- Retain old enrollment until a replacement succeeds; failure must propagate to the UI rather than being hidden by progress output.
- Do not automatically clear dictionary-attack lockout. Offer manual guidance only for explicit lockout evidence.
- This record builds on [journal attribution](0001-attribute-tpm-errors-to-disk-unlock-services.md).
