#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Read-only process observations for classic PD CLI exit contracts."""
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import socket
import signal
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('cli_config', ROOT / 'scripts/cli-config-oracle.py')
config = importlib.util.module_from_spec(spec)
spec.loader.exec_module(config)


def main():
    target = Path(os.environ['CARGO_TARGET_DIR']).resolve()
    assert target.parent == ROOT / 'target' and target.name in [f'rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / 'target/rust-slot-locks' / (target.name.replace('rust-', '') + '.lock')).is_dir()
    go = config.readonly.binary('PD_COMPAT_GO_BIN')
    rust = target / 'debug/pd-server'
    before = config.readonly.snapshot()
    record = {'observations': [], 'slot': str(target), 'before': before,
              'binaries': {kind: {'path': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
                           for kind, binary in [('go', go), ('rust', rust)]}}
    env = dict(os.environ, GOFLAGS='-mod=readonly', GOWORK='off', GOPROXY='off', GOTOOLCHAIN='local')
    cases = [(['--help'], 0), (['-h'], 0), (['help'], 0), (['--version'], 0), (['-V'], 0),
             (['--bad'], 1), (['--name'], 1), (['--config-check=no'], 1), (['--help', '--bad'], 1),
             (['--config=/does-not-exist'], 0), (['--version', '--config=/does-not-exist'], 0),
             (['--help=true', '--config=/missing'], 0), (['--version=true', '--config-check'], 0),
             (['--version=false', '--config-check'], 0), (['--version', '--version=false', '--config-check'], 0),
             (['-h=false', '--config-check'], 0), (['--config-check'], 0), (['--config-check', '--join=x', '--initial-cluster=y'], 0)]
    try:
        for args, code in cases:
            for kind, binary in [('go', go), ('rust', rust)]:
                output = subprocess.run([str(binary), *args], capture_output=True, text=True, env=env, timeout=15)
                row = dict(kind=kind, argv=output.args, code=output.returncode, stdout=output.stdout, stderr=output.stderr)
                record['observations'].append(row)
                assert output.returncode == code and not output.stderr and output.stdout, row
                if args in [['--version'], ['-V']]:
                    for field in ['Release Version:', 'Edition:', 'Kernel Type:', 'Git Commit Hash:', 'Git Branch:', 'UTC Build Time:']:
                        assert field in output.stdout
        with tempfile.TemporaryDirectory(prefix='pd-cli91-') as directory:
            with socket.socket() as occupied:
                occupied.bind(('127.0.0.1', 0))
                occupied.listen()
                client = f'http://127.0.0.1:{occupied.getsockname()[1]}'
                for kind, binary in [('go', go), ('rust', rust)]:
                    args = ['--client-urls=' + client]
                    args += ['--data-dir=' + str(Path(directory) / 'go-data')] if kind == 'go' else ['--etcd-endpoints=http://127.0.0.1:1']
                    output = subprocess.run([str(binary), *args], capture_output=True, text=True, env=env, timeout=20)
                    row = dict(kind=kind, argv=output.args, code=output.returncode, stdout=output.stdout, stderr=output.stderr)
                    record['observations'].append(row)
                    assert not output.stderr and output.returncode == 1 and 'address already in use' in (output.stdout + output.stderr).lower(), row
            record['runtime'] = config.runtime_pair(go, rust, target, directory, env)
            for name, shutdown in [('interrupt', signal.SIGINT), ('hangup', signal.SIGHUP), ('quit', signal.SIGQUIT)]:
                work = Path(directory) / name
                work.mkdir()
                record[name] = config.runtime_pair(go, rust, target, work, env, shutdown=shutdown)
    finally:
        record['after'] = config.readonly.snapshot()
        record['readonly_unchanged'] = before == record['after']
        Path(os.environ['PD_CLI_EXIT_REPORT']).write_text(json.dumps(record, indent=2))
    assert record['readonly_unchanged']
    print('18 CLI pairs, startup failure pair and HTTP and four signal pairs passed')


if __name__ == '__main__':
    main()
