#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/bin"
cat > "$tmp/bin/notify-send" <<'STUB'
#!/bin/bash
printf '%s\n' "$@" > "$TRACE"
if [[ -n ${NEW_STATUS:-} ]]; then
    cp "$NEW_STATUS" "$STATUS_FILE"
fi
echo fix
STUB
cat > "$tmp/bin/systemd-run" <<'STUB'
#!/bin/bash
printf '%s\n' "$@" > "$ROUTE"
STUB
chmod +x "$tmp/bin/notify-send" "$tmp/bin/systemd-run"
export PATH="$tmp/bin:$PATH" TRACE="$tmp/actions" ROUTE="$tmp/route"
export STATUS_FILE="$tmp/status" XDG_RUNTIME_DIR="$tmp"
cp ui/fixtures/screen4_luks_tpm.json "$STATUS_FILE"
bash bin/secureboot-notify --popup TPM_PCR_DRIFT 'TPM issue'
if grep -q 'fix=Fix now' "$TRACE"; then exit 1; fi
grep -qx details "$ROUTE"
# A stale repairable TPM status must remain blocked, too.
jq '.issues[0].auto_fixable = true' "$STATUS_FILE" > "$tmp/stale"
cp "$tmp/stale" "$STATUS_FILE"
bash bin/secureboot-notify --popup TPM_PCR_DRIFT 'TPM issue'
if grep -q 'fix=Fix now' "$TRACE"; then exit 1; fi
grep -qx details "$ROUTE"
# Preserve other repairable issues, and revalidate at the moment of clicking.
jq '.issues[0].code = "DKMS_UNSIGNED"' "$STATUS_FILE" > "$tmp/other"
cp "$tmp/other" "$STATUS_FILE"
bash bin/secureboot-notify --popup DKMS_UNSIGNED 'DKMS issue'
grep -q 'fix=Fix now' "$TRACE"
grep -qx fix "$ROUTE"
jq '.issues[0].auto_fixable = false' "$STATUS_FILE" > "$tmp/disabled"
cp "$tmp/disabled" "$STATUS_FILE"
bash bin/secureboot-notify --popup DKMS_UNSIGNED 'DKMS issue'
if grep -q 'fix=Fix now' "$TRACE"; then exit 1; fi
grep -qx details "$ROUTE"
cp "$tmp/other" "$STATUS_FILE"
printf '{"ok":true,"issues":[]}' > "$tmp/cleared"
NEW_STATUS="$tmp/cleared" bash bin/secureboot-notify --popup DKMS_UNSIGNED 'DKMS issue'
grep -qx details "$ROUTE"
echo 'Notification routing tests OK'
