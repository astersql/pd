"""Probe already built Rust PD, Go without_dashboard PD and external etcd."""

import argparse
from contextlib import ExitStack
import hashlib
import json
from pathlib import Path
import socket
import subprocess
import tempfile
import time
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, build_opener


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


HTTP = build_opener(ProxyHandler({}), NoRedirect())
HEALTH = "/pd/api/v1/health"
READY = "/pd/api/v2/ready"
VERSION = "/pd/api/v1/version"
DASHBOARD = "/dashboard/"
DASHBOARD_API = "/dashboard/api/cluster/info"
SWAGGER = "/swagger/index.html"
FALLBACK = {
    "status": 200,
    "content_type": "text/plain; charset=utf-8",
    "body": "Dashboard is not built.\n",
}


def require(condition, actual):
    if not condition:
        raise RuntimeError(repr(actual))


def addresses(count):
    with ExitStack() as stack:
        ports = []
        for _ in range(count):
            listener = stack.enter_context(socket.socket())
            listener.bind(("127.0.0.1", 0))
            ports.append(f"http://127.0.0.1:{listener.getsockname()[1]}")
        return ports


def request(base, path):
    try:
        response = HTTP.open(base + path, timeout=2)
    except HTTPError as error:
        response = error
    with response:
        return {
            "status": response.code,
            "content_type": response.headers.get("Content-Type"),
            "body": response.read().decode(),
        }


def wait_for(process, base, path, accept, log):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline and process.poll() is None:
        try:
            reply = request(base, path)
            if accept(reply):
                return reply
        except OSError:
            pass
        time.sleep(0.1)
    raise RuntimeError(f"{path} never became ready ({process.poll()})\n{log.read_text()}")


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            return process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
            raise
    return process.returncode


def launch(stack, args, directory, log):
    output = stack.enter_context(log.open("wb"))
    process = subprocess.Popen(args, cwd=directory, stdout=output, stderr=output,
                               stdin=subprocess.DEVNULL)
    stack.callback(stop, process)
    return process


def probe(kind, binary, etcd):
    with tempfile.TemporaryDirectory(prefix=f"pd-release-{kind}-") as tmp, ExitStack() as stack:
        directory = Path(tmp)
        client, peer, endpoint, etcd_peer = addresses(4)
        listeners = [client]
        if kind == "rust":
            log = directory / "etcd.log"
            store = launch(stack, [
                etcd, "--name=fixture", f"--data-dir={directory / 'etcd'}",
                f"--listen-client-urls={endpoint}", f"--advertise-client-urls={endpoint}",
                f"--listen-peer-urls={etcd_peer}", f"--initial-advertise-peer-urls={etcd_peer}",
                f"--initial-cluster=fixture={etcd_peer}", "--log-level=error",
            ], directory, log)
            wait_for(store, endpoint, "/health", lambda reply: reply["status"] == 200, log)
            listeners.extend((endpoint, etcd_peer))
            backend_args = [f"--etcd-endpoints={endpoint}"]
        else:
            listeners.append(peer)
            backend_args = [f"--initial-cluster=probe={peer}", f"--data-dir={directory / 'pd'}"]

        log = directory / "pd.log"
        pd = launch(stack, [
            binary, "--name=probe",
            f"--client-urls={client}", f"--advertise-client-urls={client}",
            f"--peer-urls={peer}", f"--advertise-peer-urls={peer}", *backend_args,
        ], directory, log)
        wait_for(pd, client, HEALTH, lambda reply: reply["status"] == 200
                 and json.loads(reply["body"])[0]["health"] is True, log)
        responses = {VERSION: wait_for(pd, client, VERSION,
                                       lambda reply: reply["status"] == 200, log)}
        require(bool(json.loads(responses[VERSION]["body"])["version"]), responses[VERSION])
        if kind == "rust":
            responses[READY] = wait_for(pd, client, READY,
                                        lambda reply: reply["status"] == 200, log)
        for path in (HEALTH, DASHBOARD, DASHBOARD_API, SWAGGER):
            responses[path] = wait_for(pd, client, path, lambda reply: True, log)
        for path in (DASHBOARD, DASHBOARD_API):
            require(responses[path] == FALLBACK, (kind, path, responses[path]))
        require(responses[SWAGGER]["status"] == 404, responses[SWAGGER])

        cli_version = subprocess.run([binary, "--version"], capture_output=True, text=True,
                                     timeout=10, check=True, cwd=directory)
        require(bool(cli_version.stdout.strip()), cli_version)
        require(pd.poll() is None, pd.returncode)
        pd_exit = stop(pd)
        require(pd_exit == 0, (kind, pd_exit, log.read_text()))
        if kind == "rust":
            stop(store)
        for url in listeners:
            with socket.socket() as listener:
                listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                listener.bind(("127.0.0.1", urlsplit(url).port))
        return {"cli_version": cli_version.stdout, "http": responses,
                "sigterm_exit": pd_exit, "released_listener_count": len(listeners)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rust", type=Path)
    parser.add_argument("go", type=Path)
    parser.add_argument("etcd", type=Path)
    args = parser.parse_args()
    binaries = {name: path.resolve(strict=True) for name, path in vars(args).items()}
    report = {"inputs": {}}
    for name, path in binaries.items():
        digest = hashlib.sha256()
        with path.open("rb") as handle:
            for block in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(block)
        report["inputs"][name] = digest.hexdigest()
    for kind in ("rust", "go"):
        report[kind] = probe(kind, binaries[kind], binaries["etcd"])
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
