# secureboot-watchdog

A small Debian package that detects when the **Secure Boot chain** on a
systemd-boot Linux machine stops being healthy — after a kernel update, a
firmware change, a DKMS rebuild, an SBAT revocation, or a TPM unlock error — and
shows a native **GNOME notification**. For repairable issues, one click starts
a polkit-elevated, issue-scoped repair flow.

Built for Ubuntu 24.04 / Zorin OS 18 / systemd-boot. Should work on any
systemd-boot + GNOME machine with the right tooling installed.

## What it detects

| Code | Meaning |
| --- | --- |
| `SB_DISABLED` | Secure Boot turned off in firmware |
| `LOCKDOWN_OFF` | Kernel lockdown is `[none]` |
| `CMDLINE_LOCKDOWN_OVERRIDE` | `lockdown=none` on the kernel cmdline |
| `UNSIGNED_MODULE_LOADED` | Tainted bit 13 set |
| `DKMS_BUILD_FAILED` | DKMS module not installed for the running kernel |
| `DKMS_UNSIGNED` | DKMS module loaded but lacks a signer |
| `SIGNATURE_FAILURES` | Kernel logged signature / lockdown rejections |
| `EFI_UNSIGNED` | Unsigned `.efi` binary on the ESP |
| `MOK_NOT_ENROLLED` | Local MOK key not enrolled in shim |
| `MOK_PENDING` | MOK enrollment queued but not completed |
| `SBAT_REVOKED` | Installed shim/systemd-boot below current SBAT revocation level |
| `TPM_PCR_DRIFT` | Cryptsetup reported an attributable TPM disk-unlock error; PCR drift is not proven |

TPM disk alerts use structured current-boot journal provenance matched to
uncommented TPM-enabled crypttab mappings, including systemd-escaped unit names.
Unrelated service credential errors do not produce a disk alert. An alert can
remain valid even if cryptsetup ultimately succeeded; it does not prove
passphrase entry, final activation failure, or a particular PCR policy change.
The issue includes structured `tpm_evidence` (mapping, configured source, unit,
message and process identity), not just prose.

Automatic TPM repair is restricted to validated supported plans. Unsupported
enrollments require manual review. Only explicit lockout evidence calls for
manual TPM lockout recovery guidance; generic argument or permission errors
do not prove lockout.

The shared read-only preflight adds `tpm_preflight` with eligibility, a reason,
and (when supported) a structured plan shown for confirmation before requesting
the existing LUKS passphrase. Root must validate the same plan before execution.
Only one affected mapping is allowed; other unrelated TPM disks are fine.
Preflight resolves the LUKS UUID and requires one TPM token and one existing
keyslot, explicit nonempty static PCRs and bank, no PIN/signed-PCR/PCRlock,
and a persistent sealing key. Unknown token fields, legacy primary-key
algorithms, ambiguous tokens, missing metadata, crypttab key files and policy
overrides all require manual review. The narrow crypttab option allowlist is
`luks`, `discard`, `nofail`, `x-initrd.attach`, `initramfs`, and `tpm2-device=auto`.

The persistent key is checked read-only using `tpm2_readpublic`: its entire
serialized identity must match `tpm2_srk`, preserving its algorithm, name and
attributes. If that tool or the required cryptenroll options are unavailable,
preflight refuses eligibility. This follows systemd 255's
[`tpm2_make_luks2_json` and serialization](https://github.com/systemd/systemd/blob/v255/src/shared/tpm2-util.c),
and tpm2-tools' [`--serialized-handle`](https://github.com/tpm2-software/tpm2-tools/blob/5.7/tools/tpm2_readpublic.c).
Test serialization is synthetic and never accesses a TPM.

The confirmation contract passes the plain JSON plan as
`--expected-tpm-plan` to the privileged helper. It includes UUID, mapping,
crypttab options, token ID and complete supported token metadata, plus explicit
PCR selection/bank and sealing-key handle. Root independently reads current-boot
journal evidence, `/etc/crypttab` and current LUKS/TPM metadata, then compares
every constraint; a different eligible target or changed token/policy is not a
substitute for the confirmed plan. There is no first-disk fallback, `TPM2_PCRS`
override or implicit 7+11 policy.

Replacement uses one `systemd-cryptenroll` invocation with TPM enrollment and
`--wipe-slot=tpm2`. The documented guarantee was checked against Ubuntu 24.04's
[systemd **255.4-1ubuntu8.18**](https://manpages.ubuntu.com/manpages/noble/man1/systemd-cryptenroll.1.html)
and Ubuntu 26.04's
[systemd-cryptsetup **259.9-0ubuntu0.1**](https://manpages.ubuntu.com/manpages/resolute/man1/systemd-cryptenroll.1.html):
enrollment completes first; wiping runs only if enrollment succeeds and always
excludes the newly added slot. This is not a promise of atomicity across power
loss or failures during old-slot cleanup. Any command/cleanup failure is reported
as failure; inspect enrollment before retrying. PCR indices/bank and sealing key
are explicit, PIN is disabled, and automatic signed-PCR/PCRlock policy discovery
is disabled. No TPM-wide lockout reset is attempted; `tpm2-tools` remains
recommended for read-only key validation and optional manual administration.

The helper reads the passphrase only from stdin, stores it briefly in a mode-600
temporary key file, and removes it on normal exit or catchable termination.
Termination stops and waits for the enrollment child before key cleanup.
SIGKILL and power loss cannot guarantee cleanup. A dry run never enrolls or
wipes slots or changes TPM state. UI cancellation is available before launch,
then disabled for the entire TPM attempt: enrollment changes must finish.
Closing the page/window or its output pipe does not kill the helper; the result
may no longer be visible. Success means **enrollment replaced**, not a verified
subsequent boot. Keep your LUKS passphrase available when rebooting.
Notification-launched UI units use `ExitType=cgroup` so the user service does
not terminate a still-running helper when the UI's main process exits. Other
repair codes retain their Cancel action.

## How it works

- `secureboot-monitor.service` runs `/usr/sbin/secureboot-check` as root and
  writes JSON to `/run/secureboot-monitor/status.json`. Driven by a timer
  (boot + hourly), a `/var/lib/dkms` path watcher, and a kernel postinst hook.
- A per-user `.path` unit watches the status file (and the notifier also
  runs at login). When the status reports `ok:false` it runs
  `/usr/bin/secureboot-notify`, which sends a GNOME notification with
  *Details* / *Dismiss* actions, plus *Fix now* only for repairable issues.
  A notification whose issue has
  cleared on a later check is withdrawn.
- *Fix now* prompts for confirmation, optionally collects a MOK enrollment
  password, then `pkexec`s `/usr/sbin/secureboot-fix --code <CODE>` to apply
  the issue-specific repair.

## Install

### Debian package

Every [release](https://github.com/Gitii/secureboot-watchdog/releases) ships
one `.deb` per supported Ubuntu, built and tested on that Ubuntu by the
`Build` workflow. Pick the file matching your release
(`….ubuntu24.04_amd64.deb` or `….ubuntu26.04_amd64.deb`) and install it:

```sh
sudo apt install ./secureboot-watchdog_*_amd64.deb
```

Log out and back in once so the user-side units come up in your session.

### From source

```sh
git clone https://github.com/Gitii/secureboot-watchdog.git
cd secureboot-watchdog
sudo apt-get build-dep ./   # build dependencies, read from debian/control
make install                # builds the .deb into dist/ and installs it with apt
```

### Uninstall

```sh
sudo apt purge secureboot-watchdog
```

## Development

```sh
make                  # build the UI (cargo build --release)
make check            # unit tests, version check
make lint             # shellcheck, polkit XML, cargo fmt + clippy
make deb              # binary .deb (dpkg-buildpackage -b), collected in dist/
make check-installed  # smoke test of the installed package
make clean
```

`make lint` runs on Ubuntu 26.04 (CI does the same) and additionally needs
`shellcheck libxml2-utils rustfmt rust-clippy`;
CI additionally needs `devscripts lintian jq`. On Ubuntu 24.04 the Rust
toolchain comes from `cargo-1.91`/`rustc-1.91`, which install outside `PATH`;
the Makefile and `debian/rules` pick them up.

`make check` fails if the version in `ui/Cargo.toml` differs from the top
entry of `debian/changelog`.

To try the UI without a real status file, run it against a fixture:

```sh
ui/target/release/secureboot-watchdog-ui --mock-status screen2_multi --mock-fix success
```

### Releasing

Bump `version` in `ui/Cargo.toml` and add an entry to the top of
`debian/changelog` (`dch -v X.Y.Z`; it is a native package, so there is no
`-1` Debian revision), merge, then tag the merge commit:

```sh
git tag v0.7.0
git push origin v0.7.0
```

The `Build` workflow (`.github/workflows/build.yml`) runs on GitHub-hosted
`ubuntu-24.04` and `ubuntu-26.04` runners. On each it:

1. installs the build dependencies with `apt-get build-dep ./`;
2. runs `make check`;
3. stamps the version with `scripts/ci-set-version.sh`;
4. runs `make deb` and `lintian --fail-on error`;
5. installs the `.deb` and runs `make check-installed`.

A separate job runs `make lint`. On a `v*` tag, a release job attaches both
`.deb` files to a GitHub release. Release packages are versioned
`X.Y.Z~ubuntu24.04` and `X.Y.Z~ubuntu26.04` (GitHub shows the `~` as `.` in
the asset names), and the script refuses a tag
that doesn't match `debian/changelog`. Pull requests, pushes to `main` and
manual runs upload the same `.deb` files as workflow artifacts, versioned
`X.Y.Z+git<date>.<sha>~ubuntuVER`.

## License

MIT — see [LICENSE](LICENSE).
