#!/usr/bin/env bash
set -Eeuo pipefail

readonly TARGET="x86_64-unknown-linux-gnu"
readonly GUI_DIR="apps/stassh-gui"
readonly GUI_BINARY="target/${TARGET}/release/stassh-gui"
readonly NATIVE_BINARY="target/release/stassh-gui"
readonly BUNDLES=("appimage" "deb" "rpm")

REQUESTED_BUNDLE="all"
declare -a SUCCEEDED_BUNDLES=()
declare -a FAILED_BUNDLES=()

usage() {
  cat <<'EOF'
Usage: ./quickstart-stassh-gui.sh [--bundle appimage|deb|rpm|all]

Analyze this system, build the amd64 stassh-gui Tauri executable, and attempt
the available Linux desktop bundles. This script is intentionally limited to
x86_64/amd64 Linux.

By default it tries: AppImage, deb, rpm.
EOF
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

warn() {
  printf 'warning: %s\n' "$*" >&2
}

info() {
  printf '%s\n' "$*"
}

optional_command_status() {
  local command_name="$1"
  if command -v "$command_name" >/dev/null 2>&1; then
    printf 'found'
  else
    printf 'missing'
  fi
}

need_command() {
  local command_name="$1"
  local description="$2"

  if ! command -v "$command_name" >/dev/null 2>&1; then
    die "$description is required but '$command_name' was not found in PATH."
  fi
}

rust_target_installed() {
  local target="$1"
  rustup target list --installed | grep -Fxq "$target"
}

ensure_repo_root() {
  [[ -f Cargo.toml ]] || die "run this script from the stassh-rust repository root."
  [[ -f "$GUI_DIR/package.json" ]] || die "$GUI_DIR/package.json was not found."
  [[ -f "$GUI_DIR/src-tauri/tauri.conf.json" ]] || die "$GUI_DIR/src-tauri/tauri.conf.json was not found."
}

ensure_base_requirements() {
  need_command cargo "Rust Cargo"
  need_command rustup "rustup"
  need_command rustc "rustc"
  need_command node "Node.js"
  need_command npm "npm"
  need_command ssh "OpenSSH client"
  need_command ssh-keygen "OpenSSH ssh-keygen"
}

ensure_amd64_linux() {
  local os arch

  os="$(uname -s 2>/dev/null || printf unknown)"
  arch="$(uname -m 2>/dev/null || printf unknown)"

  [[ "$os" == "Linux" ]] || die "GUI bundles in this script are Linux-only; detected OS: $os."
  case "$arch" in
    x86_64|amd64) ;;
    *) die "this script is limited to amd64/x86_64; detected architecture: $arch." ;;
  esac
}

ensure_rust_target() {
  if rust_target_installed "$TARGET"; then
    info "Rust target already installed: $TARGET"
    return 0
  fi

  info "Installing missing Rust target: $TARGET"
  rustup target add "$TARGET"
}

ensure_node_dependencies() {
  if [[ -d "$GUI_DIR/node_modules" ]]; then
    info "Frontend dependencies already installed: $GUI_DIR/node_modules"
    return 0
  fi

  info "Installing frontend dependencies with npm ci"
  (cd "$GUI_DIR" && npm ci)
}

print_system_report() {
  local os arch

  os="$(uname -s 2>/dev/null || printf unknown)"
  arch="$(uname -m 2>/dev/null || printf unknown)"

  info "System report"
  info "  OS/arch:        $os $arch"
  info "  cargo:          $(optional_command_status cargo)"
  info "  rustup:         $(optional_command_status rustup)"
  info "  rustc:          $(optional_command_status rustc)"
  info "  node:           $(optional_command_status node)"
  info "  npm:            $(optional_command_status npm)"
  info "  ssh:            $(optional_command_status ssh)"
  info "  ssh-keygen:     $(optional_command_status ssh-keygen)"
  info "  dpkg-deb:       $(optional_command_status dpkg-deb)"
  info "  rpmbuild:       $(optional_command_status rpmbuild)"
  info "  appimagetool:   $(optional_command_status appimagetool)"
  info
}

bundle_artifact_dir() {
  printf 'target/%s/release/bundle/%s' "$TARGET" "$1"
}

list_bundle_artifacts() {
  local bundle="$1"
  local dir

  dir="$(bundle_artifact_dir "$bundle")"
  if [[ -d "$dir" ]]; then
    find "$dir" -maxdepth 1 -type f | sort
  fi
}

build_bundle() {
  local bundle="$1"

  info
  info "Building stassh-gui bundle: $bundle"
  if (cd "$GUI_DIR" && npm run tauri -- build --target "$TARGET" --bundles "$bundle" --ci); then
    SUCCEEDED_BUNDLES+=("$bundle")
    info "Bundle complete: $bundle"
    list_bundle_artifacts "$bundle" | sed 's/^/  /'
    return 0
  fi

  local status=$?
  FAILED_BUNDLES+=("$bundle (exit $status)")
  warn "Bundle failed: $bundle"
  return "$status"
}

build_all_requested_bundles() {
  local requested_bundle="$1"
  local bundle

  if [[ "$requested_bundle" == "all" ]]; then
    for bundle in "${BUNDLES[@]}"; do
      build_bundle "$bundle" || true
    done
    return 0
  fi

  build_bundle "$requested_bundle" || true
}

print_summary() {
  local bundle

  info
  info "Summary"

  if [[ -x "$GUI_BINARY" ]]; then
    info "  Executable:"
    info "    $GUI_BINARY"
  elif [[ -x "$NATIVE_BINARY" ]]; then
    info "  Executable:"
    info "    $NATIVE_BINARY"
  fi

  if ((${#SUCCEEDED_BUNDLES[@]})); then
    info "  Bundles built:"
    for bundle in "${SUCCEEDED_BUNDLES[@]}"; do
      info "    $bundle"
      list_bundle_artifacts "$bundle" | sed 's/^/      /'
    done
  else
    info "  Bundles built: none"
  fi

  if ((${#FAILED_BUNDLES[@]})); then
    info "  Bundles failed:"
    printf '    %s\n' "${FAILED_BUNDLES[@]}"
  fi
}

parse_args() {
  while (($#)); do
    case "$1" in
      -h|--help)
        usage
        exit 0
        ;;
      --bundle)
        shift
        [[ $# -gt 0 ]] || die "--bundle requires a value."
        REQUESTED_BUNDLE="$1"
        ;;
      --bundle=*)
        REQUESTED_BUNDLE="${1#--bundle=}"
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
    shift
  done

  case "$REQUESTED_BUNDLE" in
    all|appimage|deb|rpm) ;;
    *) die "unknown bundle: $REQUESTED_BUNDLE" ;;
  esac
}

main() {
  parse_args "$@"
  ensure_repo_root
  print_system_report
  ensure_amd64_linux
  ensure_base_requirements
  ensure_rust_target
  ensure_node_dependencies

  build_all_requested_bundles "$REQUESTED_BUNDLE"
  print_summary

  if ((${#FAILED_BUNDLES[@]})); then
    exit 1
  fi
}

main "$@"
