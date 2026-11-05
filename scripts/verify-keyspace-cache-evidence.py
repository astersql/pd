#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Verify the preserved Go execution receipt and current task 026 replay inputs."""
import argparse
import copy
import gzip
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / 'docs/rust/control-plane-mcs/task-026-resume-evidence.json.gz'
TESTS = {
    'TestKeyspaceExists',
    'TestGetKeyspaceIDInRange',
    'TestRegionSpansMultipleKeyspaces',
    'TestGetKeyspaceSplitKeys',
}


def verify(data):
    receipt = data['go_receipt']
    log = receipt['raw_log']
    assert hashlib.sha256(log.encode()).hexdigest() == receipt['sha256'], 'log digest changed'
    assert receipt['checkout'] == '24c9d275e3dddf3b5976208bf159a41ceb8b8591'
    assert data['sources']['ref'] == receipt['checkout'], 'source/receipt revision mismatch'
    assert receipt['checkout'] in log, 'checkout missing from execution log'
    assert 'go version go1.26.8 linux/amd64' in log, 'Go version missing'
    xml_lines = [re.sub(r'^\S+Z ', '', line) for line in log.splitlines()]
    text = '\n'.join(xml_lines)
    start = text.index('<?xml version=')
    end = text.index('</testsuites>', start) + len('</testsuites>')
    suites = ET.fromstring(text[start:end])
    suite = next(s for s in suites if s.get('name') == 'github.com/tikv/pd/pkg/keyspace')
    assert suite.get('failures') == '0', 'Go suite failed'
    assert int(suite.get('tests')) == len(suite.findall('testcase'))
    for name in TESTS:
        cases = [t for t in suite.findall('testcase') if t.get('name') == name]
        assert len(cases) == 1, f'missing or duplicated execution: {name}'
        case = cases[0]
        assert not list(case), f'failed/skipped Go execution: {name}'
        assert float(case.get('time', '0')) > 0, f'empty execution: {name}'
    for record in data['sources']['records']:
        raw = record['source'].encode()
        expected = record['upstream_sha256']
        assert hashlib.sha256(raw).hexdigest() == expected, record['path']
        assert hashlib.sha256((ROOT / record['path']).read_bytes()).hexdigest() == expected, (
            f'Go source drift: {record["path"]}'
        )
    for path, digest in data['rust_inputs'].items():
        assert hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == digest, f'Rust input drift: {path}'
    checks = data['validation']['checks']
    assert len(checks) == 7 and all(c['exit_code'] == 0 for c in checks), 'incomplete Rust checks'
    focus = checks[1]['output']
    assert '6 passed; 0 failed; 0 ignored' in focus, 'missing real focused run'
    assert focus.count('Go span fixture ') == 14, 'incomplete span replay'
    assert focus.count('Go split fixture ') == 12, 'incomplete split replay'
    return {'go_tests': sorted(TESTS), 'span_cases': 14, 'split_cases': 12,
            'slot': data['validation']['slot'], 'historical_go_receipt': True}


def self_test(data):
    # Reject tampered receipts and stale input manifests rather than accepting an
    # aggregate CI success or a Rust implementation that changed after replay.
    def change_log(data, old, new):
        receipt = data['go_receipt']
        receipt['raw_log'] = receipt['raw_log'].replace(old, new)
        receipt['sha256'] = hashlib.sha256(receipt['raw_log'].encode()).hexdigest()

    for mutate in [
        lambda d: change_log(d, 'TestKeyspaceExists', 'MissingCase'),
        lambda d: change_log(d, 'name="TestKeyspaceExists" time="1.241651"></testcase>',
                             'name="TestKeyspaceExists" time="1.241651"><skipped/></testcase>'),
        lambda d: d['rust_inputs'].update({'pkg/keyspace/keyspace_cache.rs': '0' * 64}),
        lambda d: d['validation']['checks'][1].update(exit_code=101),
    ]:
        bad = copy.deepcopy(data)
        mutate(bad)
        try:
            verify(bad)
        except AssertionError:
            continue
        raise AssertionError('negative control unexpectedly accepted')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    with gzip.open(EVIDENCE, 'rt') as stream:
        evidence = json.load(stream)
    result = verify(evidence)
    if args.self_test:
        self_test(evidence)
        result['negative_controls'] = 4
    print(json.dumps(result, sort_keys=True))
