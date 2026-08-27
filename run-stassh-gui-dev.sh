#!/usr/bin/env bash
set -Eeuo pipefail

readonly GUI_DIR="apps/stassh-gui"
readonly DEV_DIR=".stassh-gui-dev"
readonly FIXTURE_DIR="$DEV_DIR/fixture"
readonly DEFAULT_LOG_DIR="$DEV_DIR/logs"
readonly VITE_URL="http://127.0.0.1:1420"

USE_FIXTURE=0
SIMULATION=0
NO_INSTALL=0
HEADLESS=0
LOG_DIR=""
CHILD_PID=""

usage() {
  cat <<'EOF'
Usage: ./run-stassh-gui-dev.sh [options]

Run stassh-gui in Tauri dev mode from the repository root. Tauri starts the
Vite dev server and the desktop app, and this wrapper cleans up the process
group when interrupted.

Options:
  --fixture          Use copied demo vault/local files under .stassh-gui-dev/.
  --simulation       Use in-memory demo data and simulated SSH sessions.
  --no-install       Do not run npm ci when node_modules is missing.
  --log-dir <path>   Write combined dev output to a timestamped log file.
  --headless         Run through xvfb-run -a for CI/smoke checks.
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
  [[ -f "$GUI_DIR/package.json" ]] || die "$GUI_DIR/package.json was not found."
  [[ -f "$GUI_DIR/src-tauri/tauri.conf.json" ]] || die "$GUI_DIR/src-tauri/tauri.conf.json was not found."
}

ensure_base_requirements() {
  need_command cargo "Rust Cargo"
  need_command rustc "rustc"
  need_command node "Node.js"
  need_command npm "npm"
  need_command setsid "setsid"
}

ensure_headless_requirements() {
  if ((HEADLESS)); then
    need_command xvfb-run "xvfb-run"
  fi
}

ensure_node_dependencies() {
  if [[ -d "$GUI_DIR/node_modules" ]]; then
    return 0
  fi

  if ((NO_INSTALL)); then
    die "$GUI_DIR/node_modules is missing and --no-install was provided."
  fi

  info "Installing frontend dependencies with npm ci"
  (cd "$GUI_DIR" && npm ci)
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
  info "  cargo:      $(optional_command_status cargo)"
  info "  rustc:      $(optional_command_status rustc)"
  info "  node:       $(optional_command_status node)"
  info "  npm:        $(optional_command_status npm)"
  info "  setsid:     $(optional_command_status setsid)"
  info "  xvfb-run:   $(optional_command_status xvfb-run)"
  info
}

print_launch_report() {
  info "Launching stassh-gui dev mode"
  info "  Vite URL:       $VITE_URL"
  info "  GUI dir:        $GUI_DIR"

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

  if [[ -n "$LOG_DIR" ]]; then
    mkdir -p "$LOG_DIR"
    LOG_FILE="$LOG_DIR/stassh-gui-dev-$(date +%Y%m%d-%H%M%S).log"
    info "  log:            $LOG_FILE"
  fi

  if ((HEADLESS)); then
    info "  display:        xvfb-run -a"
  fi
  info
}

cleanup() {
  local status=$?

  if [[ -n "$CHILD_PID" ]] && kill -0 "$CHILD_PID" >/dev/null 2>&1; then
    info
    info "Stopping stassh-gui dev process group: $CHILD_PID"
    kill -- "-$CHILD_PID" >/dev/null 2>&1 || kill "$CHILD_PID" >/dev/null 2>&1 || true
    wait "$CHILD_PID" >/dev/null 2>&1 || true
  fi

  exit "$status"
}

run_dev() {
  local -a command=(npm run tauri -- dev)

  if ((SIMULATION)); then
    command+=(-- -- --simulation)
  fi

  if ((HEADLESS)); then
    command=(xvfb-run -a "${command[@]}")
  fi

  if [[ -n "$LOG_DIR" ]]; then
    setsid bash -c 'cd "$1"; shift; exec "$@"' bash "$GUI_DIR" "${command[@]}" > >(tee "$LOG_FILE") 2>&1 &
  else
    setsid bash -c 'cd "$1"; shift; exec "$@"' bash "$GUI_DIR" "${command[@]}" &
  fi

  CHILD_PID=$!
  wait "$CHILD_PID"
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
      --no-install)
        NO_INSTALL=1
        ;;
      --headless)
        HEADLESS=1
        ;;
      --log-dir)
        shift
        [[ $# -gt 0 ]] || die "--log-dir requires a path."
        LOG_DIR="$1"
        ;;
      --log-dir=*)
        LOG_DIR="${1#--log-dir=}"
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

  if ((USE_FIXTURE)) && [[ -z "$LOG_DIR" ]]; then
    LOG_DIR="$DEFAULT_LOG_DIR"
  fi

  if ((USE_FIXTURE)) && ((SIMULATION)); then
    die "--fixture and --simulation cannot be used together."
  fi
}

main() {
  parse_args "$@"
  ensure_repo_root
  print_system_report
  ensure_base_requirements
  ensure_headless_requirements
  ensure_node_dependencies
  prepare_fixture
  print_launch_report

  trap cleanup INT TERM EXIT
  run_dev
}

main "$@"
