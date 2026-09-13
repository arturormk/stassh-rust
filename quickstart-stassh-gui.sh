#!/usr/bin/env bash
set -Eeuo pipefail

readonly TARGET="x86_64-unknown-linux-gnu"
readonly GUI_DIR="apps/stassh-gui"
readonly GUI_BINARY="target/${TARGET}/release/stassh-gui"
readonly NATIVE_BINARY="target/release/stassh-gui"
readonly BUNDLES=("appimage" "deb" "rpm")

REQUESTED_BUNDLE=""
EXPLICIT_BUNDLE_SELECTION=0
declare -a REQUESTED_BUNDLES=()
declare -a SUCCEEDED_BUNDLES=()
declare -a FAILED_BUNDLES=()

usage() {
  cat <<'EOF'
Usage: ./quickstart-stassh-gui.sh [--bundle appimage|deb|rpm|all|LIST]

Analyze this system, build the amd64 stassh-gui Tauri executable, and attempt
the selected Linux desktop bundles. This script is intentionally limited to
x86_64/amd64 Linux.

When --bundle is omitted, the script asks which bundle combination to build.
LIST may be comma-separated, for example: --bundle deb,rpm.
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

prompt_yes_no() {
  local prompt="$1"
  local default="${2:-n}"
  local suffix="[y/N]"
  local answer

  if [[ "$default" == "y" ]]; then
    suffix="[Y/n]"
  fi

  while true; do
    read -r -p "$prompt $suffix " answer
    answer="${answer:-$default}"
    case "$answer" in
      y|Y|yes|YES) return 0 ;;
      n|N|no|NO) return 1 ;;
      *) printf 'Please answer yes or no.\n' ;;
    esac
  done
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

package_artifacts_for_bundle() {
  local bundle="$1"
  local pattern
  local dir

  case "$bundle" in
    deb) pattern="*.deb" ;;
    rpm) pattern="*.rpm" ;;
    *) return 0 ;;
  esac

  dir="$(bundle_artifact_dir "$bundle")"
  if [[ -d "$dir" ]]; then
    find "$dir" -maxdepth 1 -type f -name "$pattern" | sort
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

bundle_requested() {
  local candidate="$1"
  local bundle

  for bundle in "${REQUESTED_BUNDLES[@]}"; do
    [[ "$bundle" == "$candidate" ]] && return 0
  done

  return 1
}

add_requested_bundle() {
  local bundle="$1"

  case "$bundle" in
    appimage|deb|rpm) ;;
    *) die "unknown bundle: $bundle" ;;
  esac

  if ! bundle_requested "$bundle"; then
    REQUESTED_BUNDLES+=("$bundle")
  fi
}

set_requested_bundles() {
  local requested_bundle="$1"
  local bundle
  local raw
  local normalized
  local -a selected=()

  REQUESTED_BUNDLES=()
  requested_bundle="${requested_bundle// /}"
  requested_bundle="${requested_bundle//$'\t'/}"
  requested_bundle="${requested_bundle,,}"
  [[ -n "$requested_bundle" ]] || die "--bundle requires a non-empty value."

  if [[ "$requested_bundle" == "all" ]]; then
    for bundle in "${BUNDLES[@]}"; do
      add_requested_bundle "$bundle"
    done
    return 0
  fi

  IFS=',' read -r -a selected <<< "$requested_bundle"
  for raw in "${selected[@]}"; do
    normalized="$raw"
    [[ -n "$normalized" ]] || die "empty bundle in selection: $requested_bundle"
    add_requested_bundle "$normalized"
  done
}

print_bundle_menu() {
  info "Choose GUI bundle output:"
  info "  1) AppImage"
  info "  2) deb"
  info "  3) rpm"
  info "  4) AppImage + deb"
  info "  5) AppImage + rpm"
  info "  6) deb + rpm"
  info "  7) AppImage + deb + rpm"
  info "  q) Quit"
  info
}

choose_bundles_interactively() {
  local choice

  print_bundle_menu
  while true; do
    read -r -p "Choose bundles to build: " choice
    case "$choice" in
      1) set_requested_bundles "appimage"; break ;;
      2) set_requested_bundles "deb"; break ;;
      3) set_requested_bundles "rpm"; break ;;
      4) set_requested_bundles "appimage,deb"; break ;;
      5) set_requested_bundles "appimage,rpm"; break ;;
      6) set_requested_bundles "deb,rpm"; break ;;
      7) set_requested_bundles "all"; break ;;
      q|Q) info "No build selected."; exit 0 ;;
      *) info "Please choose 1-7, or q to quit." ;;
    esac
  done
}

build_requested_bundles() {
  local bundle

  for bundle in "${REQUESTED_BUNDLES[@]}"; do
    build_bundle "$bundle" || true
  done
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

install_deb_package() {
  local path

  path="$(absolute_path "$1")"

  if command -v dpkg >/dev/null 2>&1; then
    if sudo dpkg -i "$path"; then
      return 0
    fi

    if command -v apt-get >/dev/null 2>&1; then
      sudo apt-get install -f -y
      return $?
    fi

    return $?
  fi

  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get install -y --reinstall "$path"
    return $?
  fi

  warn "cannot install .deb automatically because neither apt-get nor dpkg was found."
  return 1
}

install_rpm_package() {
  local path

  path="$(absolute_path "$1")"

  if command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y "$path"
    return $?
  fi

  if command -v zypper >/dev/null 2>&1; then
    sudo zypper --non-interactive install "$path"
    return $?
  fi

  if command -v rpm >/dev/null 2>&1; then
    sudo rpm -Uvh "$path"
    return $?
  fi

  warn "cannot install .rpm automatically because dnf, zypper, and rpm were not found."
  return 1
}

absolute_path() {
  local path="$1"
  local dir
  local file

  if [[ "$path" == /* ]]; then
    printf '%s\n' "$path"
    return 0
  fi

  dir="$(dirname "$path")"
  file="$(basename "$path")"
  printf '%s/%s\n' "$(cd "$dir" && pwd -P)" "$file"
}

install_package_artifact() {
  local path="$1"

  case "$path" in
    *.deb) install_deb_package "$path" ;;
    *.rpm) install_rpm_package "$path" ;;
    *) warn "unsupported package artifact: $path"; return 1 ;;
  esac
}

offer_package_install() {
  local bundle
  local artifact
  local -a packages=()

  for bundle in "${SUCCEEDED_BUNDLES[@]}"; do
    while IFS= read -r artifact; do
      packages+=("$artifact")
    done < <(package_artifacts_for_bundle "$bundle")
  done

  if ((${#packages[@]} == 0)); then
    return 0
  fi

  info
  info "Installable packages:"
  for artifact in "${packages[@]}"; do
    info "  $artifact"
  done

  if ((${#packages[@]} == 1)); then
    if prompt_yes_no "Install ${packages[0]} now?" "n"; then
      install_package_artifact "${packages[0]}"
    fi
    return 0
  fi

  info
  info "Choose a package to install:"
  local index
  local choice
  for index in "${!packages[@]}"; do
    info "  $((index + 1))) ${packages[$index]}"
  done
  info "  q) Skip install"

  while true; do
    read -r -p "Choose package to install: " choice
    case "$choice" in
      q|Q|"") info "Install skipped."; return 0 ;;
      *[!0-9]*) info "Please choose a package number, or q to skip." ;;
      *)
        if ((choice >= 1 && choice <= ${#packages[@]})); then
          install_package_artifact "${packages[$((choice - 1))]}"
          return 0
        fi
        info "Please choose a package number, or q to skip."
        ;;
    esac
  done
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
        EXPLICIT_BUNDLE_SELECTION=1
        ;;
      --bundle=*)
        REQUESTED_BUNDLE="${1#--bundle=}"
        EXPLICIT_BUNDLE_SELECTION=1
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
    shift
  done

  if ((EXPLICIT_BUNDLE_SELECTION)); then
    set_requested_bundles "$REQUESTED_BUNDLE"
  fi
}

main() {
  parse_args "$@"
  ensure_repo_root
  print_system_report
  ensure_amd64_linux
  ensure_base_requirements
  ensure_rust_target
  ensure_node_dependencies

  if ((${#REQUESTED_BUNDLES[@]} == 0)); then
    choose_bundles_interactively
  fi

  build_requested_bundles
  print_summary
  offer_package_install

  if ((${#FAILED_BUNDLES[@]})); then
    exit 1
  fi
}

main "$@"
