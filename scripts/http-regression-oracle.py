#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Observe real metrics while an existing read-only HTTP supervisor runs.

Only descendants of this invocation are inspected. This does not replay or
replace business operations: the selected supervisor owns those assertions.
"""
import argparse
import concurrent.futures
import hashlib
import http.client
import shutil
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
SCOPES = {row['metric_contract']['name']: row for row in json.loads(
    (ROOT / 'tests/rust-compat/fixtures/task-101-catalog.json').read_text())
    if 'metric_contract' in row}


def descendants(pid):
    found = []
    pending = [pid]
    while pending:
        result = subprocess.run(['pgrep', '-P', str(pending.pop())], capture_output=True, text=True)
        children = [int(p) for p in result.stdout.split()]
        found.extend(children)
        pending.extend(children)
    return found


def listeners(pid):
    result = subprocess.run([shutil.which('lsof') or 'lsof', '-nP', '-a', '-p', str(pid), '-iTCP', '-sTCP:LISTEN', '-Fn'],
                            capture_output=True, text=True)
    # Only loopback endpoints owned by a known child. Never scan arbitrary ports.
    return sorted(set('http://' + line[1:] + '/metrics' for line in result.stdout.splitlines()
                      if re.fullmatch(r'n127\.0\.0\.1:\d+', line)))


def sample_schemas(body):
    families = {line.split()[2]: dict(labels=set(), buckets=set(), enum_labels={}, scope_dependencies={}) for line in body.splitlines()
                if line.startswith('# TYPE ')}
    for line in body.splitlines():
        if not line or line.startswith('#'):
            continue
        sample = line.split('{', 1)[0].split(' ', 1)[0]
        family = sample if sample in families else re.sub(r'_(bucket|count|sum)$', '', sample)
        if family not in families:
            continue
        labels = dict(re.findall(r'([a-zA-Z_][a-zA-Z0-9_]*)="((?:\\.|[^"\\])*)"(?:,|$)',
                                 line.split('{', 1)[1].rsplit('}', 1)[0])) if '{' in line else {}
        if sample.endswith('_bucket') and 'le' in labels:
            bound = labels.pop('le')
            if bound != '+Inf':
                families[family]['buckets'].add(float(bound))
        families[family]['labels'].add(tuple(sorted(labels)))
        contract = SCOPES.get(family, {})
        dependency = next((scope for scope in contract.get('label_dependencies', [])
                           if labels.get(scope['label']) in scope['values']), None)
        if dependency:
            families[family]['scope_dependencies'][tuple(sorted(labels.items()))] = dependency['dependency']
            continue
        for key, value in labels.items():
            if key in contract.get('dynamic_label_values', {}) and re.fullmatch(r'https?://[^/]+/?', value):
                continue
            finite = {'type', 'event', 'status', 'kind', 'purpose', 'reason', 'result',
                      'group', 'runner_name', 'task_type', 'rw', 'dim', 'is-leader', 'engine', 'direction'}
            if family in {'pd_scheduler_event_count', 'pd_checker_event_count', 'pd_hotcache_status'}:
                finite.add('name')
            if family == 'service_member_role':
                finite.add('service')
            if key in finite:
                families[family]['enum_labels'].setdefault(key, set()).add(value)
    return {name: dict(labels=[list(v) for v in sorted(value['labels'])],
                       buckets=sorted(value['buckets']),
                       enum_labels={key: sorted(values) for key, values in value['enum_labels'].items()},
                       scope_dependencies=[dict(labels=dict(labels), dependency=dependency) for labels, dependency in sorted(value['scope_dependencies'].items())]) for name, value in families.items()}


def scrape(item):
    kind, pid, command, url = item
    try:
        with urllib.request.urlopen(url, timeout=0.4) as response:
            body = response.read().decode()
            if '# HELP pd_' not in body:
                return None
            return dict(implementation=kind, pid=pid, command=command, url=url,
                        time_ns=time.time_ns(), status=response.status,
                        content_type=response.headers.get('Content-Type'), exposition=body, sample_schemas=sample_schemas(body))
    except (OSError, ValueError, http.client.HTTPException):
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', required=True)
    args = parser.parse_args()
    assert re.fullmatch(r'[a-z0-9-]+', args.case)
    if args.case == 'cluster-metrics' or args.case.startswith('domain-'):
        script = ROOT / 'scripts/run-pd-compat-readonly.sh'
        case = 'gc-state' if args.case == 'domain-gc-http' else args.case
        supervisor_command = ['bash', str(script), '--require-pinned-protocol', '--case', case]
    else:
        script = ROOT / f'scripts/{args.case}-oracle.py'
        supervisor_command = [sys.executable, str(script)]
    assert script.is_file() and script.name != Path(__file__).name
    target = Path(os.environ['CARGO_TARGET_DIR'])
    assert target in [ROOT / f'target/rust-slot-{n}' for n in range(1, 11)]
    assert (ROOT / f'target/rust-slot-locks/slot-{target.name.rsplit("-", 1)[1]}.lock').is_dir()
    go = str(Path(os.environ['PD_COMPAT_GO_BIN']).resolve())
    rust = str(Path(os.environ['PD_COMPAT_RUST_BIN']).resolve())
    log = target / f'http-observation-{args.case}-{time.time_ns()}.log'
    report = log.with_suffix('.json')
    observations = {}
    child = None
    def interrupted(signum, frame):
        if child is not None and child.poll() is None:
            child.send_signal(signum)  # existing supervisor performs its own process-group cleanup
    handlers = {sig: signal.signal(sig, interrupted) for sig in (signal.SIGTERM, signal.SIGINT)}
    try:
        with log.open('wb') as output, concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            child = subprocess.Popen(supervisor_command, cwd=ROOT, stdout=output,
                                     stderr=subprocess.STDOUT)
            while child.poll() is None:
                candidates = []
                for pid in descendants(child.pid):
                    command = subprocess.run(['ps', '-p', str(pid), '-o', 'command='],
                                             capture_output=True, text=True).stdout.strip()
                    exe = command.split(' ', 1)[0]
                    if exe == go:
                        kind = 'GO'
                    elif exe == rust or (exe.startswith(str(target / 'debug/deps/astersql_pd_'))
                                          and 'fixture' not in exe):
                        kind = 'RUST'
                    else:
                        continue
                    candidates.extend((kind, pid, command, url) for url in listeners(pid))
                for result in pool.map(scrape, candidates):
                    if result is None:
                        continue
                    # Preserve first, latest and each new observable schema. Repeated timer
                    # samples do not produce unbounded per-scrape artifacts.
                    key = (result['implementation'], result['pid'], result['url'])
                    series = observations.setdefault(key, dict(first=result, last=result, schemas={}))
                    series['last'] = result
                    signature = '\n'.join(line.rsplit(' ', 1)[0] for line in result['exposition'].splitlines())
                    digest = hashlib.sha256(signature.encode()).hexdigest()
                    series['schemas'].setdefault(digest, result)
                time.sleep(0.2)
            code = child.wait()
        record = dict(case=args.case, command=supervisor_command, exit=code,
                      supervisor_log=str(log), observations=[dict(first=v['first'], last=v['last'],
                      schemas=list(v['schemas'].values())) for v in observations.values()])
        report.write_text(json.dumps(record, indent=2))
        print(log.read_text())
        print(f'HTTP_METRICS_REPORT={report}')
        return code
    finally:
        if child is not None and child.poll() is None:
            child.terminate()
            child.wait()  # do not release the Cargo slot before supervisor cleanup
        for sig, handler in handlers.items():
            signal.signal(sig, handler)


if __name__ == '__main__':
    sys.exit(main())
