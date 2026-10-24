# Attribute TPM errors to disk-unlock services

Status: accepted 2026-10-22

## The Problem

Generic systemd TPM errors can come from encrypted service credentials rather than LUKS unlock and must not trigger disk repairs.

## Options Considered

- Search all boot-journal message text for TPM failures.
- **Use structured journal attribution to associate TPM failures with configured disk-unlock services.**

## Rationale

On ms-a1, a credential-unsealing failure in distributed-hubd.service triggered a LUKS alert after cryptsetup had already completed successfully. Message text alone cannot identify the affected resource or prove PCR drift, passphrase entry, or final unlock failure.

## Notes

Keep journal provenance through detection and report only observed TPM unlock errors. A successful cryptsetup unit does not alone rule out TPM failure followed by passphrase fallback. Missing or unrelated provenance must not authorize disk repair. Explicit lockout evidence must be distinguished from generic error strings.
