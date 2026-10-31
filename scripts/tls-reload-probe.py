#!/usr/bin/env python3
# Copyright 2026 AsterSQL.
"""Real persistent and fresh TLS connections; no session resumption."""
import http.client
import pathlib
import ssl
import sys
import urllib.parse


def probe(directory, url):
    directory = pathlib.Path(directory)
    address = urllib.parse.urlsplit(url)
    original = {name: (directory / name).read_bytes() for name in ['server.pem', 'server.key', 'ca.pem']}
    def connect():
        context = ssl.create_default_context(cafile=str(directory / 'trusted.pem'))
        context.load_cert_chain(directory / 'allowed.pem', directory / 'allowed.key')
        connection = http.client.HTTPSConnection(address.hostname, address.port, context=context, timeout=3)
        connection.connect()
        return connection
    def request(connection):
        connection.request('GET', '/pd/api/v1/ping')
        response = connection.getresponse()
        assert response.status == 200
        return response.read()
    old = None
    fresh = None
    try:
        (directory / 'trusted.pem').write_bytes(original['ca.pem'])
        old = connect()
        certificate = old.sock.getpeercert(binary_form=True)
        body = request(old)
        # A different valid key pair, issued by the same CA.
        for suffix in ['pem', 'key']:
            (directory / f'server.{suffix}').write_bytes((directory / f'denied.{suffix}').read_bytes())
        fresh = connect()
        assert fresh.sock.getpeercert(binary_form=True) != certificate, 'new handshake must reload certificate'
        assert request(fresh) == body
        assert old.sock.getpeercert(binary_form=True) == certificate
        assert request(old) == body
        fresh.close()
        # CA is startup configuration, not a dynamically reloaded trust store.
        (directory / 'ca.pem').write_bytes((directory / 'rogue.pem').read_bytes())
        fresh = connect()
        assert request(fresh) == body
        fresh.close()
        for name, invalid in [('server.pem', b'invalid PEM'), ('server.key', original['server.key'])]:
            # Invalid PEM, then valid cert with a mismatched private key.
            (directory / name).write_bytes(invalid)
            if name == 'server.key':
                (directory / 'server.pem').write_bytes((directory / 'denied.pem').read_bytes())
            try:
                fresh = connect()
            except (ssl.SSLError, ConnectionError, OSError):
                pass
            else:
                fresh.close()
                raise AssertionError('bad TLS material must reject a fresh handshake')
            assert request(old) == body, 'existing connection must survive bad material'
        for name, data in original.items():
            (directory / name).write_bytes(data)
        fresh = connect()
        assert fresh.sock.getpeercert(binary_form=True) == certificate
        assert request(fresh) == body
        print('task085 rotation, static CA, invalid PEM, mismatched key, persistent connection, recovery passed')
    finally:
        for connection in [old, fresh]:
            if connection:
                connection.close()
        for name, data in original.items():
            (directory / name).write_bytes(data)
        (directory / 'trusted.pem').unlink(missing_ok=True)


if __name__ == '__main__':
    probe(*sys.argv[1:])
