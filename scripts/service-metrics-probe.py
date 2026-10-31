#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Probe real exposition and default-caller HTTP audit success/error increments."""
import http.client
import json
import re
import os
import time
import uuid
import sys
import urllib.parse


def probe(base):
    url = urllib.parse.urlsplit(base)
    def call(method, path, body=None, headers=None):
        connection = http.client.HTTPConnection(url.hostname, url.port, timeout=5)
        try:
            connection.request(method, path, body=body, headers=headers or {})
            response = connection.getresponse()
            if path.startswith('/pd/api/v1/metric/') and response.status != 429:
                assert response.getheader('Set-Cookie') is None
                assert response.getheader('Cache-Control') == 'no-store'
            return response.status, response.getheader('content-type'), response.read().decode()
        finally:
            connection.close()
    names = {
        'pd_service_maxprocs': ('gauge', 'The value of GOMAXPROCS.', set()),
        'pd_service_memory_quota_bytes': ('gauge', 'The value of memory quota bytes.', set()),
        'pd_server_info': ('gauge', 'Indicate the pd server info, and the value is the start timestamp (s).', {'version', 'hash'}),
        'pd_service_audit_handling_seconds': ('histogram', 'PD server service handling performance metrics', {'service', 'method', 'le'}),
        'pd_service_audit_requests_total': ('counter', 'Total number of service requests for audit', {'service', 'method', 'caller_component'}),
    }
    def scrape():
        status, content_type, text = call('GET', '/metrics')
        assert status == 200 and content_type.startswith('text/plain; version=0.0.4'), (status, content_type)
        return text
    def count(text, service):
        for line in text.splitlines():
            if line.startswith('pd_service_audit_requests_total{'):
                labels = dict(re.findall(r'(\w+)="([^"]*)"', line))
                if labels == dict(service=service, method='HTTP', caller_component='anonymous'):
                    return float(line.rsplit(' ', 1)[1])
        return 0
    before = scrape()
    assert call('GET', '/pd/api/v1/config')[0] == 200
    assert call('POST', '/pd/api/v1/config', '[')[0] == 400
    after = scrape()
    result = {}
    for name, (kind, help_text, label_names) in names.items():
        assert f'# HELP {name} {help_text}\n' in after, name
        assert f'# TYPE {name} {kind}\n' in after, name
        prefix = name + ('_bucket{' if kind == 'histogram' else ('{' if label_names else ' '))
        samples = [line for line in after.splitlines() if line.startswith(prefix)]
        assert samples, name
        for line in samples:
            actual = set(re.findall(r'(\w+)="', line))
            assert actual == label_names, (name, actual)
        result[name] = dict(type=kind, help=help_text, labels=sorted(label_names))
    for service in ['GetConfig', 'SetConfig']:
        delta = count(after, service) - count(before, service)
        assert delta == 1, (service, delta)
        result[service] = delta
    print('task086 metrics ' + json.dumps(result, sort_keys=True))
    queries = {}
    upstream = os.environ.get('PD_METRICS_UPSTREAM_URL')
    for method in ['GET', 'POST']:
        for route in ['query', 'query_range']:
            status, content_type, body = call(method, '/pd/api/v1/metric/' + route + '?query=rate%28x%29&x=%zz', 'a=1' if method == 'POST' else None,
                {'Connection': 'X-Drop', 'X-Drop': 'secret', 'Accept-Encoding': 'br', 'Forwarded': 'secret'})
            if upstream:
                assert status == 200 and content_type == 'application/json; charset=utf-8', (status, body)
                echo = json.loads(body)
                assert echo == dict(path='/api/v1/' + route + '?query=rate%28x%29&x=%zz', method=method, body='a=1' if method == 'POST' else '', accept='application/json', encoding='gzip', dropped=None, forwarded=None), echo
            else:
                assert status == 502 and json.loads(body)['errorType'] == 'proxy', (status, body)
            queries[method + ' ' + route] = dict(status=status, content_type=content_type, body=body)
    if upstream:
        status, content, body = call('GET', '/pd/api/v1/metric/query?mode=gzip')
        assert status == 200 and json.loads(body)['path'] == '/api/v1/query?mode=gzip', (status, body)
        for mode in ['error', 'redirect']:
            status, content, body = call('GET', '/pd/api/v1/metric/query?mode=' + mode)
            assert status == 502 and body == '{"status":"error","errorType":"proxy","error":"metric query failed"}', (status, body)
        token = uuid.uuid4().hex
        conn = http.client.HTTPConnection(url.hostname, url.port, timeout=5)
        conn.request('GET', '/pd/api/v1/metric/query?mode=slow&id=' + token)
        response = conn.getresponse()
        assert response.status == 200 and response.read(1) == b'{'
        response.close()
        conn.close()
        target = urllib.parse.urlsplit(os.environ.get('PD_METRICS_CONTROL_URL', upstream))
        deadline = time.monotonic() + 5
        while True:
            control = http.client.HTTPConnection(target.hostname, target.port, timeout=5)
            control.request('GET', '/__closed/' + token)
            closed = json.loads(control.getresponse().read())
            control.close()
            if closed:
                break
            assert time.monotonic() < deadline, 'cancelled query retained upstream connection'
            time.sleep(.02)
        queries['upstream_error_redirect_cancel'] = True
    if os.environ.get('PD_METRICS_PEAK') and upstream:
        rate = '/pd/api/v1/service-middleware/config/rate-limit'
        assert call('POST', rate, '{"type":"label","label":"queryMetric","concurrency":10}')[0] == 200
        assert call('POST', '/pd/api/v1/service-middleware/config', '{"rate-limit.enable-rate-limit":"true"}')[0] == 200
        held = []
        try:
            for _ in range(2):
                connection = http.client.HTTPConnection(url.hostname, url.port, timeout=5)
                connection.request('GET', '/pd/api/v1/metric/query?mode=slow&hold=1&id=' + uuid.uuid4().hex)
                response = connection.getresponse()
                assert response.status == 200 and response.read(1) == b'{'
                held.append((connection, response))
            deadline = time.monotonic() + 20
            while True:
                match = re.search(r'pd_server_api_concurrency\{api="queryMetric",kind="http"\} ([^\n]+)', scrape())
                if match and float(match[1]) == 2:
                    assert '# HELP pd_server_api_concurrency Concurrency number of the api.\n' in scrape()
                    assert '# TYPE pd_server_api_concurrency gauge\n' in scrape()
                    break
                assert time.monotonic() < deadline, 'concurrency metric did not measure the interval peak of two (configured limit is ten)'
                time.sleep(.05)
            assert call('POST', rate, '{"type":"label","label":"queryMetric","concurrency":2}')[0] == 200
            assert call('GET', '/pd/api/v1/metric/query')[0] == 429
        finally:
            for connection, response in held:
                response.close()
                connection.close()
        deadline = time.monotonic() + 5
        while call('GET', '/pd/api/v1/metric/query')[0] != 200:
            assert time.monotonic() < deadline, 'stream cancellation leaked rate-limit permits'
            time.sleep(.02)
        assert call('POST', '/pd/api/v1/service-middleware/config', '{"rate-limit.enable-rate-limit":"false"}')[0] == 200
        queries['concurrency_peak_and_permit_release'] = 2
    if os.environ.get('PD_METRICS_PUSH_ADDRESS'):
        job = os.environ['PD_GO_PUSH_JOB'] if base == os.environ.get('PD_METRICS_GO_URL') else os.environ['PD_METRICS_PUSH_JOB']
        deadline = time.monotonic() + 5
        while True:
            push_target = urllib.parse.urlsplit(os.environ['PD_METRICS_PUSH_ADDRESS'])
            connection = http.client.HTTPConnection(push_target.hostname, push_target.port, timeout=5)
            connection.request('GET', '/__push/' + urllib.parse.quote(job, safe=''))
            pushes = json.loads(connection.getresponse().read())
            connection.close()
            if len(pushes) >= 2:
                break
            assert time.monotonic() < deadline, 'push did not retry after gateway error'
            time.sleep(.02)
        assert pushes[0]['status'] == 503 and pushes[1]['status'] == 202
        assert all(p['method'] == 'PUT' and p['valid_metric'] and p['instance'] for p in pushes)
        print('task086 push ' + json.dumps(pushes[:2], sort_keys=True))
    print('task086 inventory ' + json.dumps(dict(queries=queries, families=re.findall(r'^# TYPE (\S+) (\S+)$', after, re.M)), sort_keys=True))
    return result


if __name__ == '__main__':
    probe(sys.argv[1])
