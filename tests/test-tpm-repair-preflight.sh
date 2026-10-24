#!/bin/bash
# Pure fixtures: no actual cryptsetup, enrollment, TPM or block-device access.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=sbin/secureboot-tpm-preflight
source sbin/secureboot-tpm-preflight
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export TPM_TEST_TRACE="$tmp/trace"
: > "$TPM_TEST_TRACE"
export TPM_TEST_METADATA="$tmp/metadata"
base=tests/fixtures/tpm-preflight/static.json
cp "$base" "$TPM_TEST_METADATA"
uuid=22222222-2222-2222-2222-222222222222
cat > "$tmp/crypttab" <<EOF
first UUID=11111111-1111-1111-1111-111111111111 none luks,tpm2-device=auto
second UUID=$uuid none luks,tpm2-device=auto
EOF
cat > "$tmp/journal" <<'EOF'
{"_SYSTEMD_UNIT":"systemd-cryptsetup@second.service","MESSAGE":"Failed to unseal secret using TPM2: Invalid argument"}
EOF
export TPM_TEST_JOURNAL="$tmp/journal"
readlink() { printf '/dev/fixture-second\n'; }
cryptsetup() {
    printf 'cryptsetup %s\n' "$*" >> "$TPM_TEST_TRACE"
    case $* in
        'luksUUID /dev/fixture-second') echo "${TPM_TEST_UUID:-22222222-2222-2222-2222-222222222222}";;
        'luksDump --dump-json-metadata /dev/fixture-second') cat "$TPM_TEST_METADATA";;
        *) echo 'unexpected cryptsetup operation' >&2; return 99;;
    esac
}
systemd-cryptenroll() {
    echo "cryptenroll $*" >> "$TPM_TEST_TRACE"
    [[ $* == --help ]] || return 99
    [[ ${TPM_TEST_NO_TOOLS:-0} == 0 ]] || return 1
    echo '--tpm2-device --tpm2-pcrs --tpm2-seal-key-handle --tpm2-with-pin --tpm2-public-key --tpm2-public-key-pcrs --tpm2-signature --tpm2-pcrlock --unlock-key-file --wipe-slot'
}
tpm2_readpublic() {
    echo "readpublic $*" >> "$TPM_TEST_TRACE"
    [[ $1 == -T && $2 == device:/dev/tpmrm0 && $3 == -c && $4 == 0x81000001 && $5 == -t ]] || return 99
    [[ ${TPM_TEST_NO_KEY:-0} == 0 ]] || return 1
    printf '%s' gQAAAQBtZXRhZGF0YS1vbmx5LXRlc3Qta2V5 | base64 -d > "$6"
}
tpm2_dictionarylockout() { echo 'MUTATION lockout' >> "$TPM_TEST_TRACE"; return 99; }
journalctl() { cat "$TPM_TEST_JOURNAL"; }
systemctl() { :; }
export -f readlink cryptsetup systemd-cryptenroll tpm2_readpublic tpm2_dictionarylockout journalctl systemctl
evidence=$(tpm_evidence "$(cat "$tmp/journal")" "$tmp/crypttab")
current=$(tpm_preflight "$evidence" "$tmp/crypttab")
jq -e --arg uuid "$uuid" '.eligible and .plan.luks_uuid == $uuid and .plan.mapping == "second"
    and .plan.token.id == "4" and .plan.token.metadata.keyslots == ["2"]
    and .plan.policy.pcrs == [4,12] and .plan.policy.bank == "sha384"
    and .plan.policy.seal_key_handle == "0x81000001"' <<< "$current" >/dev/null
expected=$(jq -c .plan <<< "$current")
tpm_plan_matches "$current" "$expected"
# Disk substitution, same-disk policy and token changes, missing fields and
# identity changes all fail the exact constraint contract.
for edit in '.luks_uuid="11111111-1111-1111-1111-111111111111"' \
    '.token.id="5"' '.token.metadata["tpm2-blob"]="bmV3"' \
    '.token.metadata["tpm2-policy-hash"]="bbbb"' '.policy.bank="sha256"' \
    '.policy.pcrs=[7,11]' '.token.metadata.tpm2_srk="bmV3"' \
    'del(.token)' '.extra="unknown"'; do
    changed=$(jq -c "$edit" <<< "$expected")
    if tpm_plan_matches "$current" "$changed"; then echo "accepted stale plan: $edit" >&2; exit 1; fi
done
if tpm_plan_matches "$current" 'not-json' 2>/dev/null; then exit 1; fi

# Changing authoritative metadata also invalidates the previously confirmed
# plan, even if the new policy is itself supported.
jq '.tokens["4"]["tpm2-pcr-bank"]="sha256"' "$base" > "$TPM_TEST_METADATA"
changed=$(tpm_preflight "$evidence" "$tmp/crypttab")
jq -e .eligible <<< "$changed" >/dev/null
if tpm_plan_matches "$changed" "$expected"; then exit 1; fi
for edit in '.tokens["5"] = .tokens["4"] | del(.tokens["4"])' \
    '.tokens["4"]["tpm2-blob"]="bmV3"'; do
    jq "$edit" "$base" > "$TPM_TEST_METADATA"
    changed=$(tpm_preflight "$evidence" "$tmp/crypttab")
    jq -e .eligible <<< "$changed" >/dev/null
    if tpm_plan_matches "$changed" "$expected"; then exit 1; fi
done
cp "$base" "$TPM_TEST_METADATA"
export TPM_TEST_UUID=33333333-3333-3333-3333-333333333333
sed "s/$uuid/$TPM_TEST_UUID/" "$tmp/crypttab" > "$tmp/changed-target"
changed_evidence=$(tpm_evidence "$(cat "$tmp/journal")" "$tmp/changed-target")
changed=$(tpm_preflight "$changed_evidence" "$tmp/changed-target")
jq -e .eligible <<< "$changed" >/dev/null
if tpm_plan_matches "$changed" "$expected"; then exit 1; fi
unset TPM_TEST_UUID

for edit in \
    '.tokens={}' '.tokens["5"] = .tokens["4"]' \
    '.tokens["5"]={"type":"other","keyslots":["2"]}' \
    '.tokens["4"]["tpm2-pin"]=true' '.tokens["4"].tpm2_pcrlock=true' \
    '.tokens["4"].tpm2_pubkey="c2lnbmVk"' '.tokens["4"].tpm2_pubkey_pcrs=[11]' \
    '.tokens["4"].tpm2_salt="c2FsdA=="' '.tokens["4"].future_security_flag=false' \
    '.tokens["4"]["tpm2-primary-alg"]="rsa"' 'del(.tokens["4"].tpm2_srk)' \
    '.tokens["4"].tpm2_srk="gQAAAWRpZmZlcmVudA=="' \
    'del(.tokens["4"]["tpm2-pcr-bank"])' '.tokens["4"]["tpm2-pcr-bank"]="unknown"' \
    '.tokens["4"]["tpm2-pcrs"]=[4,4]' '.tokens["4"]["tpm2-pcrs"]=[]' \
    '.tokens["4"]["tpm2-pcrs"]=[24]' '.tokens["4"].keyslots=["2","3"]' \
    '.keyslots={}' 'del(.tokens["4"]["tpm2-pin"])'; do
    jq "$edit" "$base" > "$TPM_TEST_METADATA"
    result=$(tpm_preflight "$evidence" "$tmp/crypttab")
    jq -e '.eligible == false and .plan == null' <<< "$result" >/dev/null || { echo "accepted $edit" >&2; exit 1; }
done
echo 'invalid json' > "$TPM_TEST_METADATA"
tpm_preflight "$evidence" "$tmp/crypttab" | jq -e '.eligible == false' >/dev/null
cp "$base" "$TPM_TEST_METADATA"
export TPM_TEST_NO_TOOLS=1
tpm_preflight "$evidence" "$tmp/crypttab" | jq -e '.eligible == false' >/dev/null
unset TPM_TEST_NO_TOOLS
export TPM_TEST_NO_KEY=1
tpm_preflight "$evidence" "$tmp/crypttab" | jq -e '.eligible == false' >/dev/null
unset TPM_TEST_NO_KEY
for option in tpm2-pcrs=7 tpm2-pin=yes tpm2-signature=/key tpm2-pcrlock=/policy \
    header=/detached key-slot=2 future-option=yes; do
    sed "s|luks,tpm2-device=auto$|luks,tpm2-device=auto,$option|" "$tmp/crypttab" > "$tmp/options"
    tpm_preflight "$evidence" "$tmp/options" | jq -e '.eligible == false' >/dev/null
done
cat "$tmp/crypttab" "$tmp/crypttab" > "$tmp/duplicate"
tpm_preflight "$evidence" "$tmp/duplicate" | jq -e '.eligible == false' >/dev/null
both=$(jq -c '. + [.[0] | .mapping="first"]' <<< "$evidence")
tpm_preflight "$both" "$tmp/crypttab" | jq -e '.eligible == false' >/dev/null
locked=$(jq -c '.[0].message="TPM2 lockout"' <<< "$evidence")
tpm_preflight "$locked" "$tmp/crypttab" | jq -e '.eligible == false' >/dev/null
tpm_preflight '[]' "$tmp/crypttab" | jq -e '.eligible == false' >/dev/null

# Integration: only a supported plan advertises repair authorization.
bash sbin/secureboot-check --tpm-fixtures "$tmp/journal" "$tmp/crypttab" > "$tmp/status"
jq -e '.issues[0] | .auto_fixable == true and .tpm_preflight.eligible
    and .tpm_preflight.plan.mapping == "second"' "$tmp/status" >/dev/null
# Direct privileged entry refuses missing authoritative host evidence.
# Test-only source copy isolates the fixed runtime lock and crypttab paths.
sed -e "s|/run/secureboot-watchdog-tpm.lock|$tmp/repair.lock|g" \
    -e "s|/etc/crypttab|$tmp/no-crypttab|g" sbin/secureboot-fix > "$tmp/secureboot-fix"
cp sbin/secureboot-tpm-preflight sbin/secureboot-tpm-replace "$tmp/"
for plan in "$expected" '{}' 'not-json'; do
    if bash "$tmp/secureboot-fix" --code TPM_PCR_DRIFT --expected-tpm-plan "$plan" --dry-run < /dev/null > "$tmp/out" 2>&1; then
        echo 'unvalidated helper succeeded' >&2; exit 1
    fi
done
if grep -E 'MUTATION|wipe|dictionarylockout' "$TPM_TEST_TRACE"; then exit 1; fi
# All enrollment invocations must be read-only help, all cryptsetup calls read-only.
if grep '^cryptenroll ' "$TPM_TEST_TRACE" | grep -v '^cryptenroll --help$'; then exit 1; fi
if grep '^cryptsetup ' "$TPM_TEST_TRACE" | grep -vE '^cryptsetup (luksUUID /dev/fixture-second|luksDump --dump-json-metadata /dev/fixture-second)$'; then exit 1; fi
echo 'TPM preflight tests OK'
