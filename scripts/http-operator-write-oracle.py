#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Run task 063 against the existing Go binary with the shared read-only guard."""
import hashlib
import importlib.util
import json
import os
import re
import signal
from pathlib import Path
import subprocess
import sys
import time
sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('readonly', ROOT / 'scripts/pd-compat-readonly.py')
readonly = importlib.util.module_from_spec(spec)
spec.loader.exec_module(readonly)

def main():
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / f'target/rust-slot-locks/slot-{target.name.rsplit("-", 1)[1]}.lock').is_dir()
    binary = readonly.binary('PD_COMPAT_GO_BIN')
    before = readonly.snapshot()
    record = {'go_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'build_info': subprocess.check_output(['go', 'version', '-m', str(binary)]).decode()}
    command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-server', '--lib', 'task_063_differential', '--', '--ignored', '--nocapture']
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off',
               CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local')
    child = None
    def interrupted(signum, frame):
        raise KeyboardInterrupt()
    previous = {sig: signal.signal(sig, interrupted) for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        child = subprocess.Popen(command, env=env, start_new_session=True,
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        stdout, _ = child.communicate()
        record.update(command=command, exit=child.returncode, output=stdout.decode())
        print(record['output'])
        counts = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', record['output'])
        assert child.returncode == 0 and counts == [('1', '0', '0')]
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)
        if child is not None:
            readonly.cleanup(child)
            record['process_group_clean'] = not readonly.group_alive(child.pid)
        record['readonly_unchanged'] = before == readonly.snapshot()
        output = target / f'http-operator-write-oracle-{time.time_ns()}.json'
        output.write_text(json.dumps(record, indent=2))
        print(output)
        assert record['readonly_unchanged']

if __name__ == '__main__':
    main()
