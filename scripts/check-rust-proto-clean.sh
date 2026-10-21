#!/usr/bin/env bash
# Copyright 2026 AsterSQL.
set -euo pipefail
cd "$(dirname "$0")/.."
case "${CARGO_TARGET_DIR:-}" in
  "$PWD"/target/rust-slot-[1-9]|"$PWD"/target/rust-slot-10) ;;
  *) echo 'claim a shared Rust slot and export its absolute CARGO_TARGET_DIR first' >&2; exit 1 ;;
esac
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
# The oracle has no failpoints; this command does not alter Go module files.
go run ./scripts/rust-proto-oracle > "$scratch/fixtures.json"
cmp pkg/proto/wire-fixtures.json "$scratch/fixtures.json"
snapshot() {
  cargo build --locked -p astersql-pd-proto --message-format=json > "$scratch/build.json"
  python3 - "$scratch/build.json" <<'PY'
import hashlib, json, pathlib, sys
outputs = [pathlib.Path(m['out_dir']) for line in open(sys.argv[1]) if (m := json.loads(line)).get('reason') == 'build-script-executed' and 'astersql-pd-proto' in m.get('package_id', '')]
assert len(outputs) == 1, outputs
for p in sorted(outputs[0].iterdir()):
    if p.suffix in ('.rs', '.bin'):
        print(p.name, hashlib.sha256(p.read_bytes()).hexdigest())
PY
}
snapshot > "$scratch/before"
# Force the pinned generator to execute again in the same output directory.
touch pkg/proto/build.rs
snapshot > "$scratch/after"
cmp "$scratch/before" "$scratch/after"
cargo metadata --locked --format-version 1 > "$scratch/metadata.json"
python3 - "$scratch/metadata.json" <<'PY'
import json, sys
packages = json.load(open(sys.argv[1]))['packages']
for name in ('prost', 'prost-types', 'tonic', 'tonic-prost', 'prometheus'):
    versions = {p['version'] for p in packages if p['name'] == name}
    assert len(versions) <= 1, (name, versions)
assert not any(p['name'] in ('protobuf', 'grpcio', 'openssl', 'openssl-sys') for p in packages)
PY
git diff --exit-code -- Cargo.toml Cargo.lock pkg/proto scripts/rust-proto-oracle scripts/check-rust-proto-clean.sh
echo 'Go fixtures, regenerated bindings/descriptors and protocol dependency versions are clean'
