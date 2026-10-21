#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mode=go
cases=()
while (($#)); do
  case "$1" in
    --mode) mode="$2"; shift 2 ;;
    --case) [[ "$2" == health-member || "$2" == tso ]] || { echo 'unknown case' >&2; exit 2; }; cases+=("$2"); shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$mode" in go|rust|dual) ;; *) echo 'invalid mode' >&2; exit 2 ;; esac
[[ -n "${CARGO_TARGET_DIR:-}" && "$CARGO_TARGET_DIR" = /* ]] || { echo 'claim a shared Cargo slot and export absolute CARGO_TARGET_DIR first' >&2; exit 2; }
slot="${CARGO_TARGET_DIR##*/rust-slot-}"
[[ "$slot" =~ ^([1-9]|10)$ && "$CARGO_TARGET_DIR" == "$PWD/target/rust-slot-$slot" && -d "target/rust-slot-locks/slot-$slot.lock" ]] || { echo 'invalid or unlocked Cargo slot' >&2; exit 2; }
# Install only the pinned tool required by this workflow. The existing make
# target still owns failpoint restoration; unrelated lint tools are unnecessary.
if [[ ! -x "$PWD/.tools/bin/failpoint-ctl" ]]; then
  GOBIN="$PWD/.tools/bin" go install github.com/pingcap/failpoint/failpoint-ctl
fi
cleanup() { make -o install-tools failpoint-disable >/dev/null; }
trap cleanup EXIT
make -o install-tools failpoint-disable >/dev/null
if [[ "$mode" != rust && -z "${PD_COMPAT_GO_BIN:-}" ]]; then
  mkdir -p bin
  CGO_ENABLED=0 go build -tags without_dashboard -o "$PWD/bin/pd-server" cmd/pd-server/main.go
  export PD_COMPAT_GO_BIN="$PWD/bin/pd-server"
fi
export PD_COMPAT_MODE="$mode"
if [[ "$mode" != go && -z "${PD_COMPAT_RUST_BIN:-}" ]]; then
  cargo build --locked -p astersql-pd-cmd-pd-server --bin pd-server
  export PD_COMPAT_RUST_BIN="$CARGO_TARGET_DIR/debug/pd-server"
fi
((${#cases[@]})) || cases=(health-member tso)
for scenario in "${cases[@]}"; do
  case "$scenario" in
    health-member) cargo test --locked -p astersql-pd-compat-tests --test health_member health_member_processes -- --nocapture ;;
    tso) cargo test --locked -p astersql-pd-compat-tests --test tso tso_processes -- --ignored --nocapture ;;
  esac
done
