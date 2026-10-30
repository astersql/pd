#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Run task 046 against the existing Go binary with the shared read-only guard."""
import hashlib
import importlib.util
import json
import os
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
    command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-server', '--lib', 'task_046_differential', '--', '--ignored', '--nocapture']
    try:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        record.update(command=command, exit=result.returncode, output=result.stdout.decode())
        print(record['output'])
        assert result.returncode == 0
    finally:
        record['readonly_unchanged'] = before == readonly.snapshot()
        output = target / f'http-cluster-oracle-{time.time_ns()}.json'
        output.write_text(json.dumps(record, indent=2))
        print(output)
        assert record['readonly_unchanged']

if __name__ == '__main__':
    main()
