#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Read-only compatibility supervisor; never downloads or rewrites Go inputs."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import signal
import shutil
import tempfile
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
MODULES = {'go.mod', 'go.sum', 'go.work', 'go.work.sum'}


def snapshot():
    files = {}
    for base, dirs, names in os.walk(ROOT):
        dirs[:] = [d for d in dirs if d != '.git']
        for name in names:
            if name.endswith('.go') or name in MODULES:
                path = Path(base) / name
                files[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    status = subprocess.check_output([
        'git', 'status', '--porcelain=v1', '--untracked-files=all', '--',
        '*.go', 'go.mod', 'go.sum', 'go.work', 'go.work.sum',
        '**/go.mod', '**/go.sum', '**/go.work', '**/go.work.sum'], cwd=ROOT).decode()
    return {'files': files, 'git': status}


def binary(name):
    value = os.environ.get(name, '')
    path = Path(value)
    if not value or not path.is_absolute() or not path.is_file() or not os.access(path, os.X_OK):
        raise ValueError(f'{name} must name an existing absolute executable')
    if path.name == 'pd-compat-different-service':
        raise ValueError('different-service is only a negative control')
    return path.resolve()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode', choices=['go', 'rust', 'dual'], default=os.environ.get('PD_COMPAT_MODE', 'dual'))
    parser.add_argument('--case', choices=['health-member', 'tso', 'cluster-host', 'cluster-restore', 'member-role', 'bucket-heartbeat', 'sync-regions', 'store-state', 'store-stats', 'store-config', 'cluster-metrics'], action='append')
    args = parser.parse_args()
    if any(case in (args.case or []) for case in ['cluster-host', 'cluster-restore', 'member-role', 'bucket-heartbeat', 'sync-regions', 'store-state', 'store-stats', 'store-config', 'cluster-metrics']) and args.mode != 'dual':
        raise ValueError('cluster-host, cluster-restore and member-role require dual mode')
    go = binary('PD_COMPAT_GO_BIN') if args.mode != 'rust' else None
    rust = binary('PD_COMPAT_RUST_BIN') if args.mode != 'go' else None
    target = Path(os.environ.get('CARGO_TARGET_DIR', ''))
    slots = [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    if not target.is_absolute() or target not in slots:
        raise ValueError('claim a shared slot and export absolute CARGO_TARGET_DIR')
    lock = ROOT / f'target/rust-slot-locks/slot-{slots.index(target)+1}.lock'
    if not lock.is_dir():
        raise ValueError('Cargo slot is not locked')
    if rust and rust != (target / 'debug/pd-server').resolve():
        raise ValueError('PD_COMPAT_RUST_BIN must be the claimed slot debug/pd-server')
    if go and rust and os.path.samefile(go, rust):
        raise ValueError('dual requires distinct Go and Rust binaries')
    fixtures = [ROOT / 'tests/rust-compat/fixtures/health-member.json']
    for fixture in fixtures:
        if not isinstance(json.loads(fixture.read_text()), dict) or not json.loads(fixture.read_text()):
            raise ValueError(f'nonempty fixture required: {fixture}')
    env = dict(os.environ, PD_COMPAT_MODE=args.mode, GOFLAGS='-mod=readonly',
               GOWORK='off', CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local')
    if go:
        info = subprocess.check_output(['go', 'version', '-m', str(go)], env=env).decode()
        if '\tdep\tgithub.com/tikv/pd\t' not in info and '\tmod\tgithub.com/tikv/pd\t' not in info:
            raise ValueError('PD_COMPAT_GO_BIN is not a Go PD binary')
    else:
        info = ''
    record = target / f'compat-readonly-{time.time_ns()}.json'
    before = snapshot()
    sources = [ROOT / name for name in (
        'tests/rust-compat/health_member_test.rs', 'tests/rust-compat/tso_test.rs',
        'tests/rust-compat/harness.rs', 'tests/rust-compat/readonly_oracle.rs',
        'scripts/run-pd-compat-readonly.sh', 'scripts/pd-compat-readonly.py',
        'pkg/proto/build.rs', 'pkg/storage/kv/etcd_fixture_test.rs', 'Cargo.lock',
        'pkg/member/member.rs', 'pkg/runtime/node_test.rs', 'pkg/runtime/member_role_worker.rs', 'pkg/runtime/member_role_worker_test.rs', 'pkg/runtime/cluster_restore.rs', 'pkg/runtime/cluster_restore_test.rs', 'pkg/server/grpc.rs',
        'pkg/server/sync_transport.rs', 'pkg/server/lib.rs', 'pkg/runtime/cluster_host.rs', 'pkg/runtime/cluster_host_test.rs', 'pkg/runtime/node.rs',
        'pkg/runtime/cluster_metrics_worker_test.rs', 'server/cluster/cluster_metrics_worker.rs', 'server/cluster/cluster_metrics_worker_test.rs', 'pkg/server/http.rs',
        'pkg/runtime/store_config_worker_test.rs', 'server/cluster/store_config_worker.rs', 'server/cluster/store_config_worker_test.rs',
        'pkg/runtime/store_stats_worker_test.rs', 'server/cluster/store_stats_worker.rs', 'server/cluster/store_stats_worker_test.rs', 'pkg/runtime/store_state_worker_test.rs', 'server/cluster/lib.rs', 'server/cluster/store_state_worker.rs', 'server/cluster/store_state_worker_test.rs', 'pkg/runtime/wire_sync_regions_test.rs', 'pkg/runtime/wire_bucket_heartbeat_test.rs', 'pkg/server/bucket_heartbeat_adapter.rs', 'server/cluster/bucket_heartbeat.rs', 'server/cluster/state_pipeline.rs', 'pkg/statistics/buckets/bucket_worker.rs', 'pkg/runtime/Cargo.toml', 'Cargo.toml')]
    report = {'baseline': subprocess.check_output(['git', 'rev-parse', 'HEAD']).decode().strip(),
              'target': str(target), 'before': before, 'go_build_info': info,
              'inputs': {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                         for p in [go, rust, *fixtures, *sources] if p}, 'commands': []}
    work = Path(tempfile.mkdtemp(prefix='compat-readonly-work-', dir=target))
    env.update(TMPDIR=str(work), PD_COMPAT_WORK_DIR=str(work))
    report['work_directory'] = str(work)
    code = 0
    child = None
    def interrupted(signum, _frame):
        raise InterruptedError(f'cancelled by signal {signum}')
    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, interrupted)
    try:
        for case in args.case or ['health-member', 'tso']:
            command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-compat-tests', '--test',
                       'health_member' if case == 'health-member' else 'tso',
                       'health_member_processes' if case == 'health-member' else 'tso_processes', '--']
            if case == 'cluster-host':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_cluster_host_differential', '--', '--ignored']
            if case == 'cluster-restore':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_cluster_restore_differential', '--', '--ignored']
            if case == 'member-role':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_member_role_differential', '--', '--ignored']
            if case == 'bucket-heartbeat':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_bucket_heartbeat_differential', '--', '--ignored']
            if case == 'sync-regions':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_sync_regions_differential', '--', '--ignored']
            if case == 'cluster-metrics':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_cluster_metrics_differential', '--', '--ignored']
            if case == 'store-config':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_store_config_differential', '--', '--ignored']
            if case == 'store-stats':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_store_stats_differential', '--', '--ignored']
            if case == 'store-state':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_store_state_differential', '--', '--ignored']
            if case == 'tso':
                command.append('--ignored')
            command.append('--nocapture')
            entry = {'argv': command}
            report['commands'].append(entry)
            log = record.with_name(f'{record.stem}-{case}.log')
            entry['log'] = str(log)
            with log.open('w') as output:
                child = subprocess.Popen(command, env=env, start_new_session=True, stdout=output, stderr=subprocess.STDOUT)
                entry['pid'] = child.pid
                code = child.wait()
            transcript = log.read_text()
            print(transcript, end='', flush=True)
            summaries = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', transcript)
            entry['test_summaries'] = summaries
            if code == 0 and summaries != [('1', '0', '0')]:
                print('expected exactly one effective scenario test', file=sys.stderr)
                code = 1
            entry['exit'] = code
            # Reap/terminate any descendants even when a test exits abnormally.
            cleanup(child)
            entry['process_group_cleaned'] = True
            child = None
            if code:
                break
    except InterruptedError as error:
        report['cancelled'] = str(error)
        code = 130
    except (OSError, subprocess.SubprocessError) as error:
        report['error'] = str(error)
        code = 2
        print(str(error), file=sys.stderr)
    finally:
        # Ignore further cancellation while terminating and recording the guard.
        for sig in (signal.SIGTERM, signal.SIGINT):
            signal.signal(sig, signal.SIG_IGN)
        cleaned = True
        if child:
            try:
                cleanup(child)
            except (OSError, subprocess.SubprocessError) as error:
                cleaned = False
                report['cleanup_error'] = str(error)
                code = 2
            report['commands'][-1]['exit'] = child.returncode
            report['commands'][-1]['process_group_cleaned'] = cleaned
        if cleaned:
            shutil.rmtree(work)
        report['work_directory_cleaned'] = not work.exists()
        after = snapshot()
        report.update(after=after, readonly_unchanged=before == after, exit=code)
        if before != after:
            code = 1
            report['exit'] = code
            print('Go read-only guard failed; changes preserved for inspection', file=sys.stderr)
        report['inputs_unchanged'] = all(Path(p).is_file() and hashlib.sha256(Path(p).read_bytes()).hexdigest() == digest for p, digest in report['inputs'].items())
        if not report['inputs_unchanged']:
            code = report['exit'] = 1
        record.write_text(json.dumps(report, sort_keys=True, indent=2) + '\n')
        print(f'read-only evidence: {record}', flush=True)
    return code


def group_alive(pgid):
    output = subprocess.check_output(['ps', '-axo', 'pgid=,stat=']).decode()
    return any(int(fields[0]) == pgid and not fields[1].startswith('Z')
               for line in output.splitlines() if len(fields := line.split()) == 2)


def cleanup(child):
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    deadline = time.monotonic() + 2
    while True:
        child.poll()  # Reap cargo before probing its group (macOS zombie groups).
        if not group_alive(child.pid):
            child.wait()
            return
        if time.monotonic() >= deadline:
            break
        time.sleep(0.05)
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        if group_alive(child.pid):
            raise
    child.wait(timeout=5)
    deadline = time.monotonic() + 5
    while group_alive(child.pid):
        if time.monotonic() >= deadline:
            raise OSError('compatibility process group did not terminate')
        time.sleep(0.05)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(2)
