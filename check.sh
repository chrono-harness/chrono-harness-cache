#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ $# -eq 0 ]]; then
  /bin/sh .chrono-harness/ci/install.sh
  export PATH="$PWD/.chrono-harness/bin:$PATH"
  export CHRONO_CHECK_SOURCE=${CHRONO_CHECK_SOURCE:-local}
  exec .chrono-harness/bin/chrono-harness check
fi
# The supervisor gives each invocation its own leased directory.
if [[ $1 == --native-in-directory && $# == 2 ]]; then
  scratch=${CHRONO_NATIVE_SCRATCH:?missing native check directory}
  shift
else
  scratch=
fi
phase=$1
case "$phase" in
  --native|build|verify) ;;
  *) echo "usage: ./check.sh [--native|build|verify]" >&2; exit 2 ;;
esac
if [[ $# -gt 1 || ( "$phase" != --native && -z "${CARGO_TARGET_DIR:-}" ) ]]; then
  echo "split native checks need a caller-owned CARGO_TARGET_DIR and a single phase" >&2
  exit 2
fi
if [[ -z "$scratch" ]]; then
  exec chrono-host-bootstrap native-scratch run /bin/bash ./check.sh --native-in-directory "$phase"
fi
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
chrono-host-bootstrap check-compiler-sources native "$stem" src/cli.rs src/retention.rs src/verified_cli.rs src/verified_planner.rs src/verified_content.rs src/verified_text.rs src/attempt.rs \
  src/recovery.rs src/content.rs src/probe.rs src/files.rs \
  src/lib.rs ||
  coverage_failure "native library differs from required sources"

# The scoped runner executes both declared proof roots as separate operations.
if [[ "$phase" == --native ]]; then exit 0; fi

src=crates/cache/src
tests=crates/cache-tests/tests
included="$tests/../../cache/src"
prove verify.cache-planner "$src/verified_planner.rs"
prove verify.cache-planner-tests "$tests/verified_planner.rs"

admit_proof verify.cache-planner "$src/verified_planner.rs" "$src/files.rs" "$src/cli.rs" "$src/retention.rs" "$src/verified_cli.rs" "$src/verified_content.rs" \
  "$src/attempt.rs" "$src/recovery.rs" "$src/probe.rs" "$src/verified_text.rs"
admit_proof verify.cache-planner-tests "$tests/verified_planner.rs" "$included/verified_planner.rs" \
  "$included/files.rs" "$included/cli.rs" "$included/retention.rs" "$included/verified_cli.rs" "$included/verified_content.rs" "$included/attempt.rs" \
  "$included/recovery.rs" "$included/probe.rs" "$included/verified_text.rs" "$tests/continuity.rs" \
  "$tests/keys.rs" "$tests/support/probe.rs" "$tests/recovery.rs" "$tests/support/cache_host.rs" "$tests/support/mock_inventory.rs"
