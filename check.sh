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
if [[ "$phase" != verify ]]; then
  cargo fmt --check --manifest-path crates/cache/Cargo.toml
  cargo fmt --check --manifest-path crates/cache-tests/Cargo.toml
  cargo build --locked --manifest-path crates/cache/Cargo.toml
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

prove() {
  local label=$1 root=$2
  local output="$scratch/$label.rlib"
  "${VERUS:-verus}" --crate-type=lib --no-cheating --no-external-by-default --num-threads 2 \
    --output-json --compile "--emit=link=$output,dep-info=$output.d" "$root" -o "$output" \
    -Zchecksum-hash-algorithm=sha256 > "$scratch/proof.json" || { cat "$scratch/proof.json" >&2; return 1; }
  cat "$scratch/proof.json"
  jq -e '.verus.version == "0.2026.04.24.f8e1704" and
    .verus.commit == "f8e17045c4ac17a02e566b8997070f2eecc255be" and
    (."verification-results" | .success == true and ."is-verifying-entire-crate" == true and
    ."encountered-error" == false and ."encountered-vir-error" == false and
    .errors == 0 and .verified > 0)' "$scratch/proof.json" >/dev/null
  test -s "$output" && test -s "$output.d"
}

prove verify.cache-planner crates/cache/src/verified_planner.rs
prove verify.cache-planner-tests crates/cache-tests/tests/verified_planner.rs
