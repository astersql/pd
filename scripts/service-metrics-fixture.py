#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Real TCP upstream for classic metric proxy and Pushgateway wire evidence."""
import base64
import http.server
import json
import gzip
import socket
import subprocess
import re
import ssl
import struct
import threading
import urllib.parse


def lan_address():
    # Route-based discovery can select a VPN's synthetic address. Select a real
    # configured interface address instead; never change host network settings.
    text = subprocess.check_output(['ifconfig'], text=True)
    candidates = []
    interface = ''
    for line in text.splitlines():
        if line and not line[0].isspace():
            interface = line.split(':', 1)[0]
        match = re.search(r'\binet (\d+\.\d+\.\d+\.\d+)', line)
        if match and (interface.startswith('en') or interface == 'lo0') and not match[1].startswith('127.'):
            candidates.append(match[1])
    assert candidates, 'a real non-127 local IPv4 address is required for the Go destination policy'
    return candidates[0]


def varint(data, at):
    value = shift = 0
    while True:
        byte = data[at]
        at += 1
        value |= (byte & 127) << shift
        if byte < 128:
            return value, at
        shift += 7
        assert shift < 70


def fields(data):
    result, at = {}, 0
    while at < len(data):
        key, at = varint(data, at)
        wire, number = key & 7, key >> 3
        if wire == 0:
            value, at = varint(data, at)
        elif wire == 1:
            value = struct.unpack('<d', data[at:at + 8])[0]
            at += 8
        elif wire == 2:
            length, at = varint(data, at)
            value = data[at:at + length]
            at += length
        elif wire == 5:
            value = data[at:at + 4]
            at += 4
        else:
            raise AssertionError('invalid protobuf wire type')
        result.setdefault(number, []).append(value)
    return result


def metric_values(data):
    result, at = {}, 0
    while at < len(data):
        length, at = varint(data, at)
        family = fields(data[at:at + length])
        at += length
        name = family[1][0].decode()
        for raw in family.get(4, []):
            metric = fields(raw)
            for kind in [2, 3]:
                if kind in metric:
                    result.setdefault(name, []).append(fields(metric[kind][0]).get(1, [0])[0])
    return result


class Backend(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    closed = set()
    pushes = {}
    lock = threading.Lock()
    def log_message(self, *args):
        pass
    def reply(self, status, body, extra=()):
        self.send_response(status)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Content-Type', 'text/plain; upstream-secret=1')
        self.send_header('Set-Cookie', 'upstream-secret=1')
        for k, v in extra:
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)
        self.wfile.flush()
    def do_GET(self):
        self.query()
    def do_POST(self):
        self.query()
    def query(self):
        path = urllib.parse.urlsplit(self.path)
        if path.path.startswith('/__closed/'):
            with self.lock:
                body = json.dumps(path.path.removeprefix('/__closed/') in self.closed).encode()
            return self.reply(200, body)
        if path.path.startswith('/__push/'):
            with self.lock:
                body = json.dumps(self.pushes.get(urllib.parse.unquote(path.path.removeprefix('/__push/')), [])).encode()
            return self.reply(200, body)
        args = urllib.parse.parse_qs(path.query)
        if args.get('mode') == ['error']:
            return self.reply(503, b'upstream-secret')
        if args.get('mode') == ['redirect']:
            return self.reply(302, b'upstream-secret', [('Location', '/forbidden-redirect')])
        if args.get('mode') == ['slow']:
            self.send_response(200)
            self.send_header('Transfer-Encoding', 'chunked')
            self.end_headers()
            self.wfile.write(b'1\r\n{\r\n')
            self.wfile.flush()
            self.connection.settimeout(25 if args.get('hold') else 5)
            try:
                ended = self.connection.recv(1) == b''
            except (ConnectionResetError, ssl.SSLEOFError):
                ended = True
            if ended:
                with self.lock:
                    self.closed.add(args['id'][0])
            self.close_connection = True
            return
        length = int(self.headers.get('Content-Length', '0'))
        body = self.rfile.read(length).decode()
        echo = dict(path=self.path, method=self.command, body=body,
                    accept=self.headers.get('Accept'), encoding=self.headers.get('Accept-Encoding'),
                    dropped=self.headers.get('X-Drop'), forwarded=self.headers.get('Forwarded'))
        payload = json.dumps(echo, sort_keys=True).encode()
        self.reply(200, gzip.compress(payload) if args.get('mode') == ['gzip'] else payload, [('Content-Encoding', 'gzip')] if args.get('mode') == ['gzip'] else [])
    def do_PUT(self):
        assert self.headers.get('Authorization') == 'Basic ZGVtbzpwQHNz', 'Pushgateway credentials were not propagated'
        parts = self.path.split('/')
        assert parts[1:3] == ['metrics', 'job@base64'], self.path
        job = base64.urlsafe_b64decode(parts[3] + '=' * (-len(parts[3]) % 4)).decode()
        instance = urllib.parse.unquote_plus(parts[5])
        body = self.rfile.read(int(self.headers['Content-Length']))
        content = self.headers['Content-Type']
        values = metric_values(body)
        assert values, 'no actual metric samples in protobuf'
        if job.startswith('oracle/rust'):
            assert values['task086_host_total'] == [7.0]
            assert values.get('pd_server_info'), values
        assert content.startswith('application/vnd.google.protobuf; proto=io.prometheus.client.MetricFamily; encoding=delimited'), content
        with self.lock:
            entries = self.pushes.setdefault(job, [])
            status = 503 if not entries else 202
            entries.append(dict(method=self.command, instance=instance, valid_metric=True, status=status, content_type=content))
        self.reply(status, b'')


def start(context=None):
    server = http.server.ThreadingHTTPServer(('0.0.0.0', 0), Backend)
    if context is not None:
        server.socket = context.wrap_socket(server.socket, server_side=True)
    server.daemon_threads = True
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, f"{'https' if context else 'http'}://{lan_address()}:{server.server_port}"
