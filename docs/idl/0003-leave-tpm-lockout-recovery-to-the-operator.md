# Leave TPM lockout recovery to the operator

Status: accepted 2026-10-22

## The Problem

Disk re-enrollment must not reset TPM-wide dictionary-attack state based on an unrelated or generic error.

## Options Considered

- Keep automatic best-effort lockout reset during disk repair.
- **Remove automatic reset and provide manual guidance only for explicit lockout evidence.**

## Rationale

The operator chose manual lockout recovery independently of the conservative disk repair boundary. Generic argument or permission errors do not prove lockout, and disk repair does not authorize TPM-wide administration.

## Notes

Retain the existing tpm2-tools recommendation for manual administration, not automatic use. Explicit lockout is not an automatic re-enrollment case. This complements [conservative enrollment repair](0002-bound-tpm-repair-to-reproducible-existing-enrollment.md).
