#!/usr/bin/env bash
# Dynamic repository-wide Cargo slots; Python owns and joins the child process group.
set -euo pipefail
exec python3 "$(dirname "$0")/slot-run.py" "$@"
