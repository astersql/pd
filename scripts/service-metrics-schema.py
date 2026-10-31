#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Compare actual classic server metric descriptors after the same RPC trace."""
from pathlib import Path
import re


def descriptors(text):
    result = {}
    lines = text.splitlines()
    for line in lines:
        if line.startswith('# HELP '):
            _, _, name, help_text = line.split(' ', 3)
            result.setdefault(name, {})['help'] = help_text
        elif line.startswith('# TYPE '):
            _, _, name, kind = line.split(' ', 3)
            result.setdefault(name, {})['type'] = kind
    for name, description in result.items():
        prefixes = (name + '{', name + ' ', name + '_bucket{', name + '_bucket ')
        samples = [line for line in lines if line.startswith(prefixes)]
        description['labels'] = sorted(set().union(*(set(re.findall(r'(\w+)="', line)) for line in samples)) - {'le'})
        if description['type'] == 'histogram':
            description['buckets'] = [('Inf' if value == float('inf') else value) for value in sorted({float(v) for line in samples for v in re.findall(r'\ble="([^"]+)"', line)})]
    return result


def compare(rust, go):
    source = (Path(__file__).resolve().parent.parent / 'server/metrics.go').read_text()
    fields = re.findall(r'Namespace:\s*"([^"]+)".*?Subsystem:\s*"([^"]+)".*?Name:\s*"([^"]+)"', source, re.S)
    assert len(fields) == 23, len(fields)
    selected = {'_'.join(field) for field in fields} | {'pd_server_info'}
    # Dedicated live tests cover audit (HTTP), interval peak and explicitly bound
    # external consensus. No fake samples are created merely to fill vectors.
    separate = {'pd_service_audit_requests_total', 'pd_service_audit_handling_seconds', 'pd_server_api_concurrency', 'pd_server_etcd_state'}
    left, right = descriptors(rust), descriptors(go)
    result = {}
    for name in sorted(selected - separate):
        assert name in left and name in right, name
        assert left[name] == right[name], (name, left[name], right[name])
        result[name] = left[name]
    assert right['pd_server_etcd_state'] == dict(help='Etcd raft states.', type='gauge', labels=['type'])
    return result
