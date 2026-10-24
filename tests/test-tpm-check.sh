#!/bin/bash
# Hardware-independent: checker fixture mode never reads the host crypttab/journal.
set -euo pipefail
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
check() {
    bash sbin/secureboot-check --tpm-fixtures "tests/fixtures/$1.jsonl" tests/fixtures/crypttab
}
for fixture in ms-a1 unattributed; do
    check "$fixture" | jq -e '.ok and .issues == []' >/dev/null
done
check cryptsetup-fallback > "$tmp/status"
jq -e '
    .issues | length == 1 and
    (.[0] | .code == "TPM_PCR_DRIFT" and .auto_fixable == false and
      (.tpm_evidence | length == 4) and
      (.tpm_evidence[0] | .mapping == "nvme0n1p3_crypt" and .source == "UUID=primary-luks" and
        .unit == "systemd-cryptsetup@nvme0n1p3_crypt.service" and .comm == "systemd-cryptse") and
      (.tpm_evidence[2] | .mapping == "data-disk" and .source == "UUID=secondary-luks" and
        .unit == "systemd-cryptsetup@data\\x2ddisk.service") and
      (.tpm_evidence[3] | .mapping == "space disk" and .source == "UUID=space-luks" and
        .unit == "systemd-cryptsetup@space\\x20disk.service") and
      (.detail | contains("manual dictionary-attack") | not))
' "$tmp/status" >/dev/null
check lockout | jq -e '.issues[0] | .auto_fixable == false and (.detail | contains("manual dictionary-attack lockout recovery"))' >/dev/null

# Direct and dry-run invocations must refuse before any sensitive command or
# stdin read. Stub the status refresh too, keeping tests independent of systemd.
mkdir "$tmp/bin"
for cmd in systemd-cryptenroll tpm2_dictionarylockout cryptsetup systemctl mktemp; do
    cat > "$tmp/bin/$cmd" <<'STUB'
#!/bin/bash
echo "${0##*/}" >> "$TRACE"
exit 0
STUB
    chmod +x "$tmp/bin/$cmd"
done
export TRACE="$tmp/trace"
for mode in normal dry; do
    args=(--code TPM_PCR_DRIFT)
    [[ $mode != dry ]] || args+=(--dry-run)
    if PATH="$tmp/bin:$PATH" bash sbin/secureboot-fix "${args[@]}" </dev/null >"$tmp/output" 2>&1; then
        echo 'TPM helper unexpectedly succeeded' >&2
        exit 1
    fi
    grep -q 'Automatic TPM repair is disabled' "$tmp/output"
done
if [[ -f $TRACE ]]; then
    if grep -E 'systemd-cryptenroll|tpm2_dictionarylockout|cryptsetup|mktemp' "$TRACE"; then exit 1; fi
fi
echo 'TPM checker/helper tests OK'
