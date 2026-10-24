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

Automatic TPM repair is currently disabled, including direct helper calls and
stale notification actions. Review the affected disk's enrollment and existing
policy manually. Only explicit lockout evidence calls for manual TPM lockout
recovery guidance; generic argument or permission errors do not prove lockout.

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
