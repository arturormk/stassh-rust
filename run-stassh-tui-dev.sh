#!/usr/bin/env bash
set -Eeuo pipefail

readonly DEV_DIR=".stassh-tui-dev"
readonly FIXTURE_DIR="$DEV_DIR/fixture"

USE_FIXTURE=0
SIMULATION=0

usage() {
  cat <<'EOF'
Usage: ./run-stassh-tui-dev.sh [options]

Run stassh-tui from source with cargo from the repository root.

Options:
  --fixture          Use copied demo vault/local files under .stassh-tui-dev/.
  --simulation       Use in-memory demo data and simulated SSH sessions.
  -h, --help         Show this help.
EOF
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

info() {
  printf '%s\n' "$*"
}

need_command() {
  local command_name="$1"
  local description="$2"

  if ! command -v "$command_name" >/dev/null 2>&1; then
    die "$description is required but '$command_name' was not found in PATH."
  fi
}

optional_command_status() {
  local command_name="$1"
  if command -v "$command_name" >/dev/null 2>&1; then
    printf 'found'
  else
    printf 'missing'
  fi
}

ensure_repo_root() {
  [[ -f Cargo.toml ]] || die "run this script from the stassh-rust repository root."
  [[ -f apps/stassh-tui/Cargo.toml ]] || die "apps/stassh-tui/Cargo.toml was not found."
}

ensure_base_requirements() {
  need_command cargo "Rust Cargo"
  need_command rustc "rustc"
}

prepare_fixture() {
  local source_dir="examples/github-screenshot"

  ((USE_FIXTURE)) || return 0

  [[ -f "$source_dir/vault.json" ]] || die "fixture vault not found: $source_dir/vault.json"
  [[ -f "$source_dir/local.json" ]] || die "fixture local config not found: $source_dir/local.json"

  mkdir -p "$FIXTURE_DIR"
  cp "$source_dir/vault.json" "$FIXTURE_DIR/vault.json"
  cp "$source_dir/local.json" "$FIXTURE_DIR/local.json"

  if [[ -f "$source_dir/secrets.json" ]]; then
    cp "$source_dir/secrets.json" "$FIXTURE_DIR/secrets.json"
  else
    rm -f "$FIXTURE_DIR/secrets.json"
  fi

  export STASSH_VAULT="$PWD/$FIXTURE_DIR/vault.json"
  export STASSH_LOCAL_CONFIG="$PWD/$FIXTURE_DIR/local.json"
  export STASSH_SECRETS="$PWD/$FIXTURE_DIR/secrets.json"
}

print_system_report() {
  info "System report"
  info "  cargo:  $(optional_command_status cargo)"
  info "  rustc:  $(optional_command_status rustc)"
  info
}

print_launch_report() {
  info "Launching stassh-tui dev mode"

  if ((SIMULATION)); then
    info "  mode:           simulation"
    info "  vault:          simulation://vault.json"
    info "  local config:   simulation://local.json"
    info "  secrets path:   simulation://secrets.json"
  elif ((USE_FIXTURE)); then
    info "  mode:           copied fixture"
    info "  vault:          $STASSH_VAULT"
    info "  local config:   $STASSH_LOCAL_CONFIG"
    info "  secrets path:   $STASSH_SECRETS"
  else
    info "  mode:           default"
    info "  vault:          default stassh path resolution"
    info "  local config:   default stassh path resolution"
    info "  secrets path:   default stassh path resolution"
  fi

  info
}

run_dev() {
  local -a command=(cargo run -p stassh-tui --)

  if ((SIMULATION)); then
    command+=(--simulation)
  fi

  exec "${command[@]}"
}

parse_args() {
  while (($#)); do
    case "$1" in
      --fixture)
        USE_FIXTURE=1
        ;;
      --simulation)
        SIMULATION=1
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
    shift
  done

  if ((USE_FIXTURE)) && ((SIMULATION)); then
    die "--fixture and --simulation cannot be used together."
  fi
}

main() {
  parse_args "$@"
  ensure_repo_root
  print_system_report
  ensure_base_requirements
  prepare_fixture
  print_launch_report

  run_dev
}

main "$@"
