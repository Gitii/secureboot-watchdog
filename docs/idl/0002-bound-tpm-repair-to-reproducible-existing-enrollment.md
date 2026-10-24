# Bound TPM repair to reproducible existing enrollment

Status: accepted 2026-10-22

## The Problem

Repair must not guess the affected disk, silently change its TPM policy, or destroy working enrollment before replacement succeeds.

## Options Considered

- Keep first-disk selection, default PCRs 7+11, and preemptive slot deletion.
- **Repair only an unambiguous affected disk with a supported, reproducible existing policy; otherwise require manual recovery.**
- Build a disk and enrollment-policy selection UI.

## Rationale

Conservative repair keeps the existing workflow without expanding it into a policy-management tool. Re-enrollment is not permission to change security policy.

## Notes

- Revalidate disk identity and enrollment as root at repair time, rather than trusting UI data or choosing the first crypttab entry.
- Bind execution to the disk and policy confirmed in the UI through an expected plan identity; root independently recomputes the plan and rejects mismatches. Use a canonical representation of disk UUID and relevant token/policy parameters, not a new hashing protocol. These values are constraints, not authority.
- Preserve existing policy parameters; never substitute PCRs 7+11 or an environment override.
- Unsupported or ambiguous enrollment gets guidance and no enrollment changes.
- Retain old enrollment until a replacement succeeds; failure must propagate to the UI rather than being hidden by progress output.
- Do not expose repair until safe execution is present. Disable UI cancellation for a launched TPM repair, rather than force-killing a privileged enrollment process. Allow cancellation before launch; window closure must not kill the helper. Catchable external termination must stop/wait for children before cleanup; SIGKILL and power-loss cleanup cannot be guaranteed.
- Lockout recovery follows [its separate decision](0003-leave-tpm-lockout-recovery-to-the-operator.md).
- This record builds on [journal attribution](0001-attribute-tpm-errors-to-disk-unlock-services.md).
