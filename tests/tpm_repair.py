"""Unprivileged model of systemd's documented enroll-before-wipe guarantee."""
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

ROOT = Path.cwd()
SECRET = "test-passphrase-not-for-argv"
STATUS = json.loads((ROOT / "ui/fixtures/screen4_luks_tpm.json").read_text())
PLAN = STATUS["issues"][0]["tpm_preflight"]["plan"]
PLAN["mapping"] = "second"

STUB = r'''#!/usr/bin/python3
import base64, json, os, pathlib, signal, stat, sys, time
p = pathlib.Path(os.environ['CASE'])
name = pathlib.Path(sys.argv[0]).name
a = sys.argv[1:]
with (p/'trace').open('a') as f: f.write(json.dumps([name] + a) + '\n')
if name == 'readlink':
    print('/dev/fixture-second')
elif name == 'journalctl':
    print(json.dumps({'_SYSTEMD_UNIT':'systemd-cryptsetup@second.service',
        'MESSAGE':'Failed to unseal secret using TPM2: Invalid argument'}))
elif name == 'cryptsetup':
    if a == ['luksUUID','/dev/fixture-second']:
        print('22222222-2222-2222-2222-222222222222')
    elif a == ['luksDump','--dump-json-metadata','/dev/fixture-second']:
        print((p/'metadata').read_text())
    else: sys.exit(98)
elif name == 'tpm2_readpublic':
    pathlib.Path(a[-1]).write_bytes(base64.b64decode('gQAAAQBtZXRhZGF0YS1vbmx5LXRlc3Qta2V5'))
elif name == 'systemd-cryptenroll':
    if a == ['--help']:
        print('--tpm2-device --tpm2-pcrs --tpm2-seal-key-handle --tpm2-with-pin --tpm2-public-key --tpm2-public-key-pcrs --tpm2-signature --tpm2-pcrlock --unlock-key-file --wipe-slot')
        sys.exit(0)
    key = pathlib.Path(next(x.split('=',1)[1] for x in a if x.startswith('--unlock-key-file=')))
    assert stat.S_IMODE(key.stat().st_mode) == 0o600
    assert key.read_text() == 'test-passphrase-not-for-argv'
    assert set(a) == {'--tpm2-device=/dev/tpmrm0','--tpm2-pcrs=4:sha384+12:sha384',
        '--tpm2-seal-key-handle=0x81000001','--tpm2-with-pin=no','--tpm2-public-key=',
        '--tpm2-public-key-pcrs=','--tpm2-signature=','--tpm2-pcrlock=',
        '--unlock-key-file='+str(key),'--wipe-slot=tpm2','/dev/fixture-second'}
    (p/'child').write_text(str(os.getpid()))
    (p/'key-path').write_text(str(key))
    def terminate(sig, frame):
        assert key.exists(), 'key removed before child stopped'
        (p/'stopped').touch()
        sys.exit(128+sig)
    signal.signal(signal.SIGTERM, terminate)
    (p/'ready').touch()
    time.sleep(float(os.environ.get('DELAY','0')))
    mode = os.environ.get('MODE','success')
    if mode in ('wrong-passphrase','enroll-failure'): sys.exit(9)
    # Enrollment succeeded: failure of the following wipe is still failure.
    (p/'state').write_text('old,new')
    if mode == 'wipe-failure': sys.exit(5)
    (p/'state').write_text('new')
elif name == 'mktemp':
    if a and a[0].startswith('/run/secureboot-tpm-key.'):
        os.execv('/usr/bin/mktemp', ['mktemp', str(p/'unlock.XXXXXXXX')])
    if a == ['-d']:
        os.execv('/usr/bin/mktemp', ['mktemp', '-d', str(p/'preflight.XXXXXXXX')])
    os.execv('/usr/bin/mktemp', ['mktemp'] + a)
elif name == 'rm':
    if os.environ.get('MODE') == 'preflight-cleanup-failure' and any(x.endswith('/srk') for x in a):
        sys.exit(1)
    if any('/unlock.' in x for x in a):
        pid = int((p/'child').read_text()) if (p/'child').exists() else None
        if pid:
            try: os.kill(pid, 0)
            except ProcessLookupError: pass
            else: sys.exit('cleanup ran while child was alive')
        (p/'cleanup').touch()
        if os.environ.get('MODE') == 'key-cleanup-failure': sys.exit(1)
    os.execv('/usr/bin/rm', ['rm'] + a)
elif name == 'systemctl': pass
else: sys.exit('forbidden command: ' + name)
'''


def wait_ready(p, proc):
    deadline = time.monotonic() + 10
    while not (p / "ready").exists():
        assert proc.poll() is None, "helper exited before child ready"
        assert time.monotonic() < deadline, "child startup timeout"
        time.sleep(0.01)


def case(mode="success", action=None, dry=False, expected=PLAN, edit=None, send_secret=True):
    with tempfile.TemporaryDirectory(prefix="tpm-repair-") as td:
        p = Path(td)
        (p / "bin").mkdir()
        for name in ["readlink", "journalctl", "cryptsetup", "tpm2_readpublic",
                     "systemd-cryptenroll", "mktemp", "rm", "systemctl",
                     "tpm2_dictionarylockout"]:
            stub = p / "bin" / name
            stub.write_text(STUB)
            stub.chmod(0o755)
        (p / "state").write_text("old")
        metadata = json.loads((ROOT / "tests/fixtures/tpm-preflight/static.json").read_text())
        if edit:
            edit(metadata)
        (p / "metadata").write_text(json.dumps(metadata))
        (p / "crypttab").write_text(
            "first UUID=11111111-1111-1111-1111-111111111111 none luks,tpm2-device=auto\n"
            "second UUID=22222222-2222-2222-2222-222222222222 none luks,tpm2-device=auto\n")
        # Test-only copy: production has no configurable privileged fixture paths.
        script = (ROOT / "sbin/secureboot-fix").read_text()
        script = script.replace("/etc/crypttab", str(p / "crypttab"))
        (p / "secureboot-fix").write_text(script)
        for lib in ["secureboot-tpm-preflight", "secureboot-tpm-replace"]:
            (p / lib).write_text((ROOT / "sbin" / lib).read_text())
        env = dict(os.environ, CASE=td, MODE=mode, DELAY="0.5" if action else "0",
                   PATH=str(p / "bin") + ":" + os.environ["PATH"], TPM2_PCRS="7+11")
        args = ["bash", str(p / "secureboot-fix"), "--code", "TPM_PCR_DRIFT",
                "--expected-tpm-plan", json.dumps(expected)]
        if dry:
            args.append("--dry-run")
        proc = subprocess.Popen(args, env=env, stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        try:
            if send_secret:
                proc.stdin.write((SECRET + "\n").encode())
            proc.stdin.close()
            proc.stdin = None
            if action:
                wait_ready(p, proc)
                if action == "close-reader":
                    proc.stdout.close()
                    proc.stdout = None
                elif action == "hup":
                    proc.send_signal(signal.SIGHUP)
                elif action == "int":
                    proc.send_signal(signal.SIGINT)
                else:
                    proc.send_signal(signal.SIGTERM)
            output = proc.communicate(timeout=10)[0] or b""
            rc = proc.returncode
            valid = expected == PLAN and not edit and mode != "preflight-cleanup-failure"
            success = valid and (dry or (send_secret and mode == "success" and action not in ("term", "int")))
            assert (rc == 0) == success, (mode, action, rc, output)
            assert SECRET.encode() not in output
            trace = (p / "trace").read_text()
            assert SECRET not in trace and "dictionarylockout" not in trace
            calls = [json.loads(x) for x in trace.splitlines()]
            enroll = [x for x in calls if x[0] == "systemd-cryptenroll" and x[1:] != ["--help"]]
            assert len(enroll) == int(valid and not dry and send_secret)
            if valid and not dry and send_secret:
                child = int((p / "child").read_text())
                try:
                    os.kill(child, 0)
                except ProcessLookupError:
                    pass
                else:
                    raise AssertionError("lingering enrollment child")
                assert (p / "cleanup").exists()
                keys = list(p.glob("unlock.*"))
                assert bool(keys) == (mode == "key-cleanup-failure")
            else:
                assert not list(p.glob("unlock.*"))
            state = (p / "state").read_text()
            if mode in ("wrong-passphrase", "enroll-failure") or action in ("term", "int") or dry or not valid or not send_secret:
                assert state == "old"
            elif mode == "wipe-failure":
                assert state == "old,new"
            else:
                assert state == "new"
            if success and not dry and action != "close-reader":
                assert b"TPM enrollment replaced" in output
            else:
                assert b"TPM enrollment replaced" not in output
        finally:
            if proc.poll() is None:
                proc.terminate()
                proc.wait(timeout=10)


for scenario in ["success", "wrong-passphrase", "enroll-failure", "wipe-failure", "key-cleanup-failure", "preflight-cleanup-failure"]:
    case(scenario)
case(dry=True)
case(send_secret=False)
for interruption in ["term", "int", "hup", "close-reader"]:
    case(action=interruption)
case(expected={})
case(expected=dict(PLAN, luks_uuid="11111111-1111-1111-1111-111111111111"))
case(edit=lambda m: m["tokens"]["4"].update({"tpm2-pcr-bank": "sha256"}))
print("TPM replacement process tests OK")
