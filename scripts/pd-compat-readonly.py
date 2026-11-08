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
import socket
import urllib.request
import shutil
import tempfile
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
MODULES = {'go.mod', 'go.sum', 'go.work', 'go.work.sum'}


# These are the complete dual matrices, rather than their Rust-only admission tests.
GC_CASES = {
    'gc-legacy': ('accept_gc_03_test::sa_003_behavior', 'SA_003_REPORT'),
    'gc-v2': ('accept_gc_04_test::sa_004_behavior', 'PD_SA004_REPORT'),
    'gc-advance': ('accept_gc_05_test::sa_005_dual_matrix', 'PD_SA005_REPORT'),
    'gc-barrier': ('accept_gc_06_test::sa_006_dual_matrix', 'PD_SA006_REPORT'),
    'gc-global-barrier': ('accept_gc_07_test::sa_007_dual_matrix', 'PD_SA007_REPORT'),
    'gc-state': ('accept_gc_08_test::sa_008_dual_matrix', 'PD_SA008_REPORT'),
}


def validate_go_build_info(info):
    """Reject unrelated binaries and protocol drift before starting an oracle."""
    records = [line.split() for line in info.splitlines()]
    if not any(len(r) >= 3 and r[0] in ('dep', 'mod') and
               r[1] == 'github.com/tikv/pd' for r in records):
        raise ValueError('oracle is not a Go PD artifact')
    if not any(len(r) >= 3 and r[:3] == [
            'dep', 'github.com/pingcap/kvproto',
            'v0.0.0-20261016101550-f8ad079c8e98'] for r in records):
        raise ValueError('oracle must use pinned kvproto f8ad079c8e98')


def start_storage_fixture(target, directory, env):
    """Start a caller-owned fixture without building Go when a pinned binary exists."""
    native = env.get('PD_COMPAT_ETCD_BIN')
    if native:
        binary = Path(native)
        assert binary.is_absolute() and binary.is_file()
        version = subprocess.check_output([str(binary), '--version'], env=env).decode()
        assert 'etcd Version: 3.5.15' in version.splitlines(), version
        listeners = [socket.socket(), socket.socket()]
        try:
            for listener in listeners:
                listener.bind(('127.0.0.1', 0))
            client, peer = [f'http://127.0.0.1:{s.getsockname()[1]}' for s in listeners]
        finally:
            for listener in listeners:
                listener.close()
        command = [str(binary), '--name=default', f'--data-dir={directory / "data"}',
                   f'--listen-client-urls={client}', f'--advertise-client-urls={client}',
                   f'--listen-peer-urls={peer}', f'--initial-advertise-peer-urls={peer}',
                   f'--initial-cluster=default={peer}', '--log-level=error']
    else:
        fixtures = sorted(target.glob('rust-etcd-fixture-*'), key=lambda p: p.stat().st_mtime)
        assert fixtures, 'set PD_COMPAT_ETCD_BIN or build the read-only fixture first'
        binary = fixtures[-1]
        command = [str(binary), str(directory)]
    directory.mkdir()
    with (directory / 'fixture.log').open('wb') as output:
        child = subprocess.Popen(command, stdin=subprocess.DEVNULL if native else subprocess.PIPE,
                                 stdout=output, stderr=output, env=env, start_new_session=True)
    try:
        deadline = time.monotonic() + 25
        while True:
            assert child.poll() is None, (directory / 'fixture.log').read_text()
            if native:
                try:
                    with urllib.request.urlopen(client + '/health', timeout=0.5) as response:
                        if response.status == 200:
                            return child, binary, client
                except OSError:
                    pass
            elif (directory / 'ready').exists():
                return child, binary, (directory / 'ready').read_text()
            assert time.monotonic() < deadline, 'fixture readiness timeout'
            time.sleep(0.05)
    except BaseException:
        cleanup(child)
        raise


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
    parser.add_argument('--require-pinned-protocol', action='store_true',
                        help='require the exact protocol revision for full wire acceptance')
    parser.add_argument('--mode', choices=['go', 'rust', 'dual'], default=os.environ.get('PD_COMPAT_MODE', 'dual'))
    parser.add_argument('--case', choices=['health-member', 'tso', 'cluster-host', 'cluster-restore', 'member-role', 'bucket-heartbeat', 'sync-regions', 'store-state', 'store-stats', 'store-config', 'cluster-metrics', 'resolved-ts', 'schedule-coordinator', 'request-role', 'members', 'bootstrap', 'alloc-id', 'store-registration', 'region-lookup', 'region-scan', 'region-query', 'ask-split', 'report-split', 'cluster-config', 'scatter', 'split-regions', 'split-scatter', 'global-config', 'process-shutdown', 'rolling-restart', 'go-client-core', 'embedded-isolation', 'shutdown-workers', 'global-watch', 'report-min-resolved-ts', 'external-timestamp', 'classic-tso', 'deprecated-rpc', 'diagnostics-info', *GC_CASES], action='append')
    args = parser.parse_args()
    if any(case in (args.case or []) for case in ['cluster-host', 'cluster-restore', 'member-role', 'bucket-heartbeat', 'sync-regions', 'store-state', 'store-stats', 'store-config', 'cluster-metrics', 'resolved-ts', 'schedule-coordinator', 'request-role', 'members', 'bootstrap', 'alloc-id', 'store-registration', 'region-lookup', 'region-scan', 'region-query', 'ask-split', 'report-split', 'cluster-config', 'scatter', 'split-regions', 'split-scatter', 'global-config', 'process-shutdown', 'rolling-restart', 'go-client-core', 'embedded-isolation', 'shutdown-workers', 'global-watch', 'report-min-resolved-ts', 'external-timestamp', 'classic-tso', 'deprecated-rpc', 'diagnostics-info', *GC_CASES]) and args.mode != 'dual':
        raise ValueError('selected compatibility cases require dual mode')
    gc_selected = bool(GC_CASES.keys() & set(args.case or []))
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
    if 'diagnostics-info' in (args.case or []):
        fixtures.append(ROOT / 'tests/rust-compat/fixtures/diagnostics-info-darwin-amd64.json')
    for fixture in fixtures:
        if not isinstance(json.loads(fixture.read_text()), dict) or not json.loads(fixture.read_text()):
            raise ValueError(f'nonempty fixture required: {fixture}')
    env = dict(os.environ, PD_COMPAT_MODE=args.mode, GOFLAGS='-mod=readonly',
               GOWORK='off', CGO_ENABLED='0', GOPROXY='off', GOTOOLCHAIN='local')
    if go:
        info = subprocess.check_output(['go', 'version', '-m', str(go)], env=env).decode()
        if '\tdep\tgithub.com/tikv/pd\t' not in info and '\tmod\tgithub.com/tikv/pd\t' not in info:
            raise ValueError('PD_COMPAT_GO_BIN is not a Go PD binary')
        if args.require_pinned_protocol or gc_selected:
            validate_go_build_info(info)
    else:
        info = ''
    etcd = None
    if gc_selected or (args.require_pinned_protocol and 'go-client-core' in (args.case or [])):
        etcd = binary('PD_COMPAT_ETCD_BIN')
        version = subprocess.check_output([str(etcd), '--version'], env=env).decode()
        if 'etcd Version: 3.5.15' not in version.splitlines():
            raise ValueError('compatibility cases require official etcd 3.5.15')
    record = target / f'compat-readonly-{time.time_ns()}.json'
    before = snapshot()
    sources = [ROOT / name for name in (
        'scripts/pd-compat-readonly.py', 'tests/rust-compat/oracle_runner.rs',
        'tests/rust-compat/oracle_runner_test.rs', 'tests/rust-compat/readonly_oracle.rs',
        'tests/rust-compat/harness.rs',
        'tests/rust-compat/accept_diagnostics_info_test.rs', 'pkg/mcs/server/diagnostics_info.rs',
        'pkg/mcs/server/diagnostics_info_test.rs', 'pkg/mcs/server/diagnostics_log.rs',
        'tests/rust-compat/accept_http_09_test.rs', 'pkg/server/accept_http_09.rs', 'pkg/server/grpc_keyspace.rs', 'pkg/runtime/control_keyspace.rs', 'pkg/keyspace/keyspace_watch.rs',
        'tests/rust-compat/accept_gc_08_test.rs', 'pkg/server/grpc_gc_read.rs', 'pkg/server/http_gc_read.rs',
        'tests/rust-compat/accept_gc_05_test.rs', 'pkg/server/grpc_gc_advance.rs',
        'tests/rust-compat/accept_gc_03_test.rs', 'pkg/server/grpc_gc_legacy.rs',
        'tests/rust-compat/accept_gc_04_test.rs', 'pkg/server/grpc_gc_v2.rs',
        'tests/rust-compat/accept_gc_06_test.rs', 'pkg/server/grpc_gc_barrier.rs',
        'tests/rust-compat/accept_gc_07_test.rs',
        'tests/rust-compat/grpc_regression.rs', 'tests/rust-compat/grpc_regression_test.rs',
        'docs/rust/server-api/wire-inventory.md',
        'pkg/server/grpc_unsupported.rs', 'pkg/server/grpc_unsupported_test.rs', 'pkg/server/grpc_unsupported_scenarios_test.rs', 'pkg/runtime/grpc_unsupported_differential_test.rs',
        'pkg/server/grpc_tso.rs', 'pkg/server/grpc_tso_test.rs', 'pkg/server/grpc_tso_scenarios_test.rs', 'pkg/runtime/grpc_tso_differential_test.rs', 'pkg/runtime/grpc_tso_clock_test.rs', 'pkg/runtime/tso.rs',
        'pkg/server/grpc_external_ts.rs', 'pkg/server/grpc_external_ts_test.rs', 'pkg/server/grpc_external_ts_scenarios_test.rs', 'pkg/runtime/grpc_external_ts_differential_test.rs', 'pkg/runtime/grpc_external_ts_fault_test.rs',
        'pkg/server/grpc_resolved_ts.rs', 'pkg/server/grpc_resolved_ts_test.rs', 'pkg/server/grpc_resolved_ts_scenarios_test.rs', 'pkg/runtime/grpc_resolved_ts_differential_test.rs', 'pkg/runtime/cluster_host.rs',
        'cmd/pd-server/process_shutdown_test.rs', 'cmd/pd-server/main.rs', 'cmd/pd-server/lib.rs', 'cmd/pd-server/Cargo.toml',
        'pkg/runtime/shutdown_workers_test.rs', 'pkg/runtime/embedded_isolation_test.rs',
        'pkg/server/grpc_global_watch.rs', 'pkg/server/grpc_global_watch_test.rs', 'pkg/server/grpc_global_watch_scenarios_test.rs', 'pkg/runtime/grpc_global_watch_differential_test.rs',
        'pkg/storage/kv/memory_store.rs', 'pkg/storage/kv/contract_test.rs', 'pkg/storage/kv/meta_store_test.rs', 'pkg/storage/kv/etcd_store_test.rs',
        'pkg/server/grpc_global_config.rs', 'pkg/server/grpc_global_config_test.rs', 'pkg/server/grpc_global_config_scenarios_test.rs', 'pkg/runtime/grpc_global_config_differential_test.rs',
        'pkg/server/grpc_split_scatter.rs', 'pkg/server/grpc_split_scatter_test.rs', 'pkg/server/grpc_split_scatter_scenarios_test.rs', 'pkg/runtime/grpc_split_scatter_differential_test.rs', 'pkg/runtime/grpc_split_scatter_fault_test.rs',
        'pkg/server/grpc_split_regions.rs', 'pkg/server/grpc_split_regions_test.rs', 'pkg/server/grpc_split_regions_scenarios_test.rs', 'pkg/runtime/schedule_split.rs', 'pkg/runtime/grpc_split_regions_clock_test.rs', 'pkg/runtime/grpc_split_regions_differential_test.rs', 'pkg/schedule/splitter/split_retry.rs',
        'pkg/server/grpc_scatter.rs', 'pkg/server/grpc_scatter_test.rs', 'pkg/server/grpc_scatter_scenarios_test.rs', 'pkg/runtime/schedule_scatter.rs', 'pkg/runtime/grpc_scatter_fault_test.rs', 'pkg/runtime/schedule_worker.rs', 'pkg/runtime/grpc_scatter_differential_test.rs',
        'pkg/server/grpc_region_query.rs', 'pkg/server/grpc_region_query_test.rs', 'pkg/server/grpc_region_query_scenarios_test.rs', 'pkg/runtime/grpc_region_query_differential_test.rs',
        'pkg/server/grpc_region_scan.rs', 'pkg/server/grpc_region_scan_test.rs', 'pkg/server/grpc_region_scan_scenarios_test.rs', 'pkg/runtime/grpc_region_scan_differential_test.rs', 'pkg/runtime/grpc_region_scan_fault_test.rs', 'pkg/core/regions_batch_scan.rs',
        'tests/rust-compat/health_member_test.rs', 'tests/rust-compat/tso_test.rs',
        'tests/rust-compat/harness.rs', 'tests/rust-compat/readonly_oracle.rs',
        'tests/rust-compat/rolling_restart.rs', 'tests/rust-compat/rolling_restart_test.rs',
        'tests/rust-compat/go_client_acceptance.rs', 'tests/rust-compat/go_client_acceptance_test.rs',
        'scripts/run-pd-compat-readonly.sh', 'scripts/pd-compat-readonly.py',
        'pkg/proto/build.rs', 'pkg/storage/kv/etcd_fixture_test.rs', 'Cargo.lock',
        'pkg/member/member.rs', 'pkg/runtime/node_test.rs', 'pkg/runtime/member_role_worker.rs', 'pkg/runtime/member_role_worker_test.rs', 'pkg/runtime/cluster_restore.rs', 'pkg/runtime/cluster_restore_test.rs', 'pkg/server/grpc.rs',
        'pkg/storage/kv/meta_store.rs', 'pkg/storage/kv/etcd_store.rs',
        'pkg/runtime/lib.rs', 'pkg/server/core_store_adapter.rs', 'pkg/server/core_region_adapter.rs', 'pkg/storage/endpoint/endpoint_codec.rs', 'pkg/storage/endpoint/lib.rs',
        'pkg/server/grpc_store.rs', 'pkg/server/grpc_store_test.rs', 'pkg/server/grpc_store_scenarios_test.rs', 'pkg/runtime/grpc_store_differential_test.rs', 'pkg/runtime/grpc_store_fault_test.rs',
        'pkg/server/grpc_cluster_config.rs', 'pkg/server/grpc_cluster_config_test.rs', 'pkg/server/grpc_cluster_config_scenarios_test.rs', 'pkg/runtime/grpc_cluster_config_differential_test.rs',
        'pkg/server/grpc_report_split.rs', 'pkg/server/grpc_report_split_test.rs', 'pkg/server/grpc_report_split_scenarios_test.rs', 'pkg/runtime/grpc_report_split_differential_test.rs',
        'pkg/server/grpc_ask_split.rs', 'pkg/server/grpc_ask_split_test.rs', 'pkg/server/grpc_ask_split_scenarios_test.rs', 'pkg/runtime/cluster_split.rs', 'pkg/runtime/grpc_ask_split_differential_test.rs',
        'pkg/server/grpc_alloc_id.rs', 'pkg/server/grpc_alloc_id_test.rs', 'pkg/runtime/grpc_alloc_id_differential_test.rs', 'pkg/id/allocator.rs',
        'pkg/server/grpc_bootstrap.rs', 'pkg/server/grpc_bootstrap_test.rs', 'pkg/runtime/grpc_bootstrap_differential_test.rs', 'pkg/storage/endpoint/cluster_meta.rs', 'pkg/storage/endpoint/store_meta.rs', 'pkg/storage/endpoint/region_meta.rs',
        'pkg/server/grpc_members.rs', 'pkg/server/grpc_members_test.rs', 'pkg/runtime/grpc_members_differential_test.rs',
        'pkg/server/request_role.rs', 'pkg/server/request_role_test.rs', 'pkg/runtime/request_role_test.rs',
        'pkg/server/sync_transport.rs', 'pkg/server/lib.rs', 'pkg/runtime/cluster_host.rs', 'pkg/runtime/cluster_host_test.rs', 'pkg/runtime/node.rs',
        'pkg/runtime/schedule_worker.rs', 'pkg/runtime/schedule_policy.rs', 'pkg/runtime/schedule_worker_test.rs', 'pkg/server/heartbeat_stream_adapter.rs', 'pkg/core/regions_shared.rs', 'pkg/schedule/schedulers/scheduler_control.rs', 'pkg/schedule/schedulers/hot_schedule.rs', 'pkg/schedule/schedulers/hot_solve.rs', 'pkg/schedule/schedulers/hot_source.rs', 'server/config/persist_schedule.rs',
        'pkg/runtime/resolved_ts_worker_test.rs', 'server/cluster/resolved_ts_worker.rs', 'server/cluster/resolved_ts_worker_test.rs',
        'pkg/runtime/cluster_metrics_worker_test.rs', 'server/cluster/cluster_metrics_worker.rs', 'server/cluster/cluster_metrics_worker_test.rs', 'pkg/server/http.rs',
        'pkg/runtime/store_config_worker_test.rs', 'server/cluster/store_config_worker.rs', 'server/cluster/store_config_worker_test.rs',
        'pkg/runtime/store_stats_worker_test.rs', 'server/cluster/store_stats_worker.rs', 'server/cluster/store_stats_worker_test.rs', 'pkg/runtime/store_state_worker_test.rs', 'server/cluster/lib.rs', 'server/cluster/store_state_worker.rs', 'server/cluster/store_state_worker_test.rs', 'pkg/runtime/wire_sync_regions_test.rs', 'pkg/runtime/wire_bucket_heartbeat_test.rs', 'pkg/server/bucket_heartbeat_adapter.rs', 'server/cluster/bucket_heartbeat.rs', 'server/cluster/state_pipeline.rs', 'pkg/statistics/buckets/bucket_worker.rs', 'pkg/runtime/Cargo.toml', 'Cargo.toml')]
    report = {'baseline': subprocess.check_output(['git', 'rev-parse', 'HEAD']).decode().strip(),
              'target': str(target), 'before': before, 'go_build_info': info,
              'inputs': {str(p): hashlib.sha256(p.read_bytes()).hexdigest()
                         for p in [go, rust, etcd, *fixtures, *sources] if p}, 'commands': []}
    if 'go-client-core' in (args.case or []):
        for name in ('pd-region-bench', 'pd-tso-bench', 'pd-heartbeat-bench'):
            tool = target / name
            if not tool.is_file() or not os.access(tool, os.X_OK):
                raise ValueError(f'existing Go client tool required: {tool}')
            build = subprocess.check_output(['go', 'version', '-m', str(tool)], env=env).decode()
            if '\tmod\tgithub.com/tikv/pd/tools\t' not in build:
                raise ValueError(f'not a Go PD tools binary: {tool}')
            if args.require_pinned_protocol:
                if not any(line.split()[:3] == [
                        'dep', 'github.com/pingcap/kvproto',
                        'v0.0.0-20261016101550-f8ad079c8e98'] for line in build.splitlines()):
                    raise ValueError(f'Go tool must use pinned kvproto f8ad079c8e98: {tool}')
            report.setdefault('go_client_build_info', {})[name] = build
            report['inputs'][str(tool)] = hashlib.sha256(tool.read_bytes()).hexdigest()
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
            evidence_path = None
            case_env = env.copy()
            if case in GC_CASES:
                test, report_var = GC_CASES[case]
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-compat-tests',
                           '--lib', test, '--', '--exact']
                evidence_path = work / f'{case}.json'
                case_env[report_var] = str(evidence_path)
            if case == 'diagnostics-info':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-compat-tests', '--lib',
                           'sa_015_server_info', '--']
            if case == 'alloc-id':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_alloc_id_differential', '--', '--ignored']
            if case == 'bootstrap':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_bootstrap_differential', '--', '--ignored']
            if case == 'members':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_members_differential', '--', '--ignored']
            if case == 'request-role':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_request_role_differential', '--', '--ignored']
            if case == 'cluster-host':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_cluster_host_differential', '--', '--ignored']
            if case == 'region-lookup':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_region_lookup_differential', '--', '--ignored']
            if case == 'deprecated-rpc':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_deprecated_rpc_differential', '--', '--ignored']
            if case == 'classic-tso':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_classic_tso_differential', '--', '--ignored']
            if case == 'external-timestamp':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_external_timestamp_differential', '--', '--ignored']
            if case == 'report-min-resolved-ts':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_report_min_resolved_ts_differential', '--', '--ignored']
            if case == 'go-client-core':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-compat-tests', '--lib',
                           'task_099_', '--']
            if case == 'rolling-restart':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-compat-tests', '--lib',
                           'rolling_restart_test::task_098_behavior', '--', '--exact']
            if case == 'process-shutdown':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-cmd-pd-server', '--lib',
                           'process_shutdown_test::task_097_readonly_differential',
                           '--', '--ignored', '--exact']
            if case == 'embedded-isolation':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'embedded_isolation_test::task_096_readonly_differential',
                           '--', '--ignored', '--exact']
            if case == 'shutdown-workers':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'shutdown_workers_test::task_095_readonly_shutdown_differential',
                           '--', '--ignored', '--exact']
            if case == 'global-watch':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_global_watch_differential', '--', '--ignored']
            if case == 'global-config':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_global_config_differential', '--', '--ignored']
            if case == 'split-scatter':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_split_scatter_differential', '--', '--ignored']
            if case == 'split-regions':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_split_regions_differential', '--', '--ignored']
            if case == 'scatter':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_scatter_differential', '--', '--ignored']
            if case == 'cluster-config':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_cluster_config_differential', '--', '--ignored']
            if case == 'report-split':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_report_split_differential', '--', '--ignored']
            if case == 'ask-split':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_ask_split_differential', '--', '--ignored']
            if case == 'region-query':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_region_query_differential', '--', '--ignored']
            if case == 'region-scan':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_region_scan_differential', '--', '--ignored']
            if case == 'store-registration':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_store_registration_differential', '--', '--ignored']
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
            if case == 'schedule-coordinator':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_schedule_coordinator_differential', '--', '--ignored']
            if case == 'resolved-ts':
                command = ['cargo', 'test', '--locked', '-p', 'astersql-pd-runtime', '--lib',
                           'readonly_resolved_ts_differential', '--', '--ignored']
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
            entry = {'case': case, 'argv': command}
            report['commands'].append(entry)
            log = record.with_name(f'{record.stem}-{case}.log')
            entry['log'] = str(log)
            with log.open('w') as output:
                child = subprocess.Popen(command, env=case_env, start_new_session=True, stdout=output, stderr=subprocess.STDOUT)
                entry['pid'] = child.pid
                code = child.wait()
            transcript = log.read_text()
            print(transcript, end='', flush=True)
            summaries = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', transcript)
            entry['test_summaries'] = summaries
            if code == 0 and summaries != [('1', '0', '0')]:
                print('expected exactly one effective scenario test', file=sys.stderr)
                code = 1
            if evidence_path is not None and code == 0:
                evidence = json.loads(evidence_path.read_text())
                if not isinstance(evidence, (dict, list)) or not evidence:
                    raise ValueError(f'{case} did not record RPC and persisted state')
                entry['evidence'] = evidence
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
    except (ValueError, OSError, subprocess.SubprocessError) as error:
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
