#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
phase=${1:-all}
case "$phase" in
  all|build|verify) ;;
  *) echo "usage: ./check.sh [build|verify]" >&2; exit 2 ;;
esac
if [[ $# -gt 1 || ( "$phase" != all && -z "${CARGO_TARGET_DIR:-}" ) ]]; then
  echo "split native checks need a caller-owned CARGO_TARGET_DIR and a single phase" >&2
  exit 2
fi
mkdir -p target
scratch=$(mktemp -d "$(pwd -P)/target/check.XXXXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
export TMPDIR="$scratch"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$scratch/cargo}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR=$(cd "$CARGO_TARGET_DIR" && pwd -P)
product=crates/cache/Cargo.toml
if [[ "$phase" != verify ]]; then
  cargo fmt --check --manifest-path crates/cache/Cargo.toml
  cargo fmt --check --manifest-path crates/cache-tests/Cargo.toml
fi
cargo build --locked --message-format=json-render-diagnostics --manifest-path "$product" > "$scratch/build.jsonl"
if [[ "$phase" != verify ]]; then
  cargo test --no-run --locked --manifest-path crates/cache-tests/Cargo.toml
  for program in chrono-harness chrono-diagnostics; do
    binding="$CARGO_TARGET_DIR/debug/$program"
    if [[ "$(readlink "$binding" || true)" != chrono-cache-mock-owner ]]; then
      ln -s chrono-cache-mock-owner "$binding"
    fi
  done
  if [[ "$phase" == build ]]; then exit 0; fi
fi
PATH="$CARGO_TARGET_DIR/debug:$PATH" cargo test --locked --manifest-path crates/cache-tests/Cargo.toml -- --test-threads=2


coverage_failure() {
  echo "E_VERUS_COVERAGE: $*" >&2
  exit 1
}

prove() {
  chrono-host-bootstrap run-compiler-proof "$scratch" "$1" "${VERUS:-verus}" "$2" ||
    coverage_failure "$1: verifier execution failed"
}

admit_proof() {
  local operation=$1
  shift
  chrono-host-bootstrap collect-compiler-proof "$scratch" "$operation" "${VERUS:-verus}" \
    0.2026.04.24.f8e1704 f8e17045c4ac17a02e566b8997070f2eecc255be "$@" ||
    coverage_failure "$operation: required proof execution was not admitted"
}

# Read the exact library selected by Cargo in the build of the tested executable.
metadata=$(jq -ers --arg manifest "$(pwd -P)/$product" '
  [.[] | select(.reason == "compiler-artifact" and .manifest_path == $manifest
                 and .target.kind == ["lib"]) | .filenames[] | select(endswith(".rmeta"))]
  | if length == 1 then .[0] else error("expected the product library") end
' "$scratch/build.jsonl") || coverage_failure "native library was not identified"
filename=${metadata##*/}
stem=${filename#lib}
stem=${metadata%/*}/${stem%.rmeta}
chrono-host-bootstrap check-compiler-sources native "$stem" src/verified_planner.rs src/verified_content.rs src/attempt.rs \
  src/recovery.rs src/content.rs src/probe.rs \
  src/lib.rs ||
  coverage_failure "native library differs from required sources"

src=crates/cache/src
tests=crates/cache-tests/tests
included="$tests/../../cache/src"
prove verify.cache-planner "$src/verified_planner.rs"
prove verify.cache-planner-tests "$tests/verified_planner.rs"

admit_proof verify.cache-planner "$src/verified_planner.rs" "$src/verified_content.rs" \
  "$src/attempt.rs" "$src/recovery.rs"
admit_proof verify.cache-planner-tests "$tests/verified_planner.rs" "$included/verified_planner.rs" \
  "$included/verified_content.rs" "$included/attempt.rs" \
  "$included/recovery.rs" "$tests/continuity.rs" \
  "$tests/keys.rs" "$tests/support/probe.rs"
