#!/usr/bin/env bash
set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

once=0
auto=0
require_audit=1
max_iterations=0
iteration=1

usage() {
  cat <<'EOF'
Usage: scripts/review-loop.sh [options]

Runs code + security review checks in a loop until all checks pass.

Options:
  --once             Run one pass only (exit non-zero on failure)
  --auto             Re-run automatically without waiting for Enter
  --max N            Stop after N failed iterations
  --require-audit    Require `cargo audit` to be installed (default)
  --allow-missing-audit
                    Allow skipping audit when `cargo audit` is unavailable
  -h, --help         Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --once)
      once=1
      shift
      ;;
    --auto)
      auto=1
      shift
      ;;
    --max)
      if [[ $# -lt 2 ]]; then
        echo "error: --max requires a number" >&2
        exit 2
      fi
      max_iterations="$2"
      shift 2
      ;;
    --require-audit)
      require_audit=1
      shift
      ;;
    --allow-missing-audit)
      require_audit=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option '$1'" >&2
      usage >&2
      exit 2
      ;;
  esac
done

has_cargo_audit() {
  cargo audit --version >/dev/null 2>&1
}

run_review_pass() {
  echo
  echo "== Review iteration $iteration =="
  echo "-- Code checks"
  cargo fmt --all --check || return 1
  cargo clippy --all-targets --all-features -- -D warnings || return 1
  cargo test --quiet || return 1

  echo "-- Security checks"
  if has_cargo_audit; then
    cargo audit || return 1
  elif [[ "$require_audit" -eq 1 ]]; then
    echo "cargo audit is required but not installed." >&2
    echo "Install with: cargo install cargo-audit" >&2
    return 1
  else
    echo "Skipping cargo audit (not installed)."
  fi

  return 0
}

while true; do
  if run_review_pass; then
    echo
    echo "✅ Review loop passed. Everyone should be happy now."
    exit 0
  fi

  if [[ "$once" -eq 1 ]]; then
    echo
    echo "❌ Review checks failed."
    exit 1
  fi

  if [[ "$max_iterations" -gt 0 && "$iteration" -ge "$max_iterations" ]]; then
    echo
    echo "❌ Reached max iterations ($max_iterations) with failing checks."
    exit 1
  fi

  ((iteration++))

  if [[ "$auto" -eq 1 ]]; then
    continue
  fi

  echo
  read -r -p "Fix issues, then press Enter to rerun (q to quit): " answer
  if [[ "${answer,,}" == "q" ]]; then
    exit 1
  fi
done
