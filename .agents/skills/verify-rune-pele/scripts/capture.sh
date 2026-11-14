#!/bin/sh
# Runs one command and keeps its proof: the command line, stdout, stderr and exit code.
# usage: capture.sh <run-id> <feature-id> <command> [args...]
# Proof lands in out/verify/<run-id>/<feature-id>/ and the command's exit code is passed through.
set -u
[ $# -ge 3 ] || { echo "usage: capture.sh <run-id> <feature-id> <command> [args...]" >&2; exit 64; }
root=$(cd "$(dirname "$0")/../../../.." && pwd)
run=$1 feature=$2
shift 2
dir="$root/out/verify/$run/$feature"
mkdir -p "$dir"
n=$(find "$dir" -name '*.cmd' | wc -l | tr -d ' ')
printf '%s\n' "$*" > "$dir/$n.cmd"
(cd "$root" && mise exec -- "$@") > "$dir/$n.stdout" 2> "$dir/$n.stderr"
code=$?
echo "$code" > "$dir/$n.exit"
echo "capture: $dir/$n exit $code"
exit "$code"
