# secureboot-watchdog

A small Debian package that detects when the **Secure Boot chain** on a
systemd-boot Linux machine stops being healthy — after a kernel update, a
firmware change, a DKMS rebuild, an SBAT revocation, or a TPM PCR shift — and
shows a native **GNOME notification**. One click runs a polkit-elevated,
issue-scoped repair.

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
| `TPM_PCR_DRIFT` | TPM2 LUKS unlock failed (PCR mismatch) |

## How it works

- `secureboot-monitor.service` runs `/usr/sbin/secureboot-check` as root and
  writes JSON to `/run/secureboot-monitor/status.json`. Driven by a timer
  (boot + hourly), a `/var/lib/dkms` path watcher, and a kernel postinst hook.
- A per-user `.path` unit watches the status file. On change with `ok:false`
  it runs `/usr/bin/secureboot-notify`, which sends a GNOME notification with
  *Fix now* / *Details* / *Dismiss* actions.
- *Fix now* prompts for confirmation, optionally collects a MOK enrollment
  password, then `pkexec`s `/usr/sbin/secureboot-fix --code <CODE>` to apply
  the issue-specific repair.

## Install

```sh
sudo apt install ./secureboot-watchdog_0.1_all.deb
```

Log out and back in once so the user-side `.path` unit comes up in your
running session.

## Build from source

```sh
sudo apt install debhelper devscripts lintian
debuild -us -uc -b
sudo apt install ../secureboot-watchdog_0.1_all.deb
```

## Uninstall

```sh
sudo apt purge secureboot-watchdog
```

## License

MIT — see [LICENSE](LICENSE).
