#!/usr/bin/env bash
set -Eeuo pipefail

readonly RUSTFLAGS_RELEASE="-C debuginfo=0 -C strip=symbols"
readonly INSTALL_DIR="/usr/local/bin"
readonly BINARIES=("stassh" "stassh-tui")

declare -a SUCCEEDED_TARGETS=()
declare -a FAILED_TARGETS=()

usage() {
  cat <<'EOF'
Usage: ./quickstart-stassh-tui.sh

Analyze this system, choose a stassh/stassh-tui build target, install missing
Rust targets or Debian/Ubuntu cross compiler packages when needed, then build
the selected binaries.
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

is_debian_like() {
  command -v apt-get >/dev/null 2>&1 && command -v dpkg >/dev/null 2>&1
}

package_installed() {
  local package="$1"
  dpkg -s "$package" >/dev/null 2>&1
}

rust_target_installed() {
  local target="$1"
  rustup target list --installed | grep -Fxq "$target"
}

target_output_dir() {
  local target="$1"

  if [[ -z "$target" ]]; then
    printf 'target/release'
  else
    printf 'target/%s/release' "$target"
  fi
}

binary_paths_for_target() {
  local target="$1"
  local output_dir

  output_dir="$(target_output_dir "$target")"
  printf '%s/stassh\n' "$output_dir"
  printf '%s/stassh-tui\n' "$output_dir"
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
  info "  ssh:            $(optional_command_status ssh)"
  info "  ssh-keygen:     $(optional_command_status ssh-keygen)"

  if is_debian_like; then
    info "  package tool:   apt-get/dpkg"
  else
    info "  package tool:   unsupported for automatic cross-linker install"
  fi

  info
  info "Installed Rust targets"
  if command -v rustup >/dev/null 2>&1; then
    rustup target list --installed | sed 's/^/  /'
  else
    info "  rustup not found"
  fi

  info
  info "Cross linkers"
  info "  i686-linux-gnu-gcc:       $(optional_command_status i686-linux-gnu-gcc)"
  info "  arm-linux-gnueabihf-gcc:  $(optional_command_status arm-linux-gnueabihf-gcc)"
  info
}

ensure_repo_root() {
  [[ -f Cargo.toml ]] || die "run this script from the stassh-rust repository root."
  grep -q 'members = \[' Cargo.toml || die "Cargo.toml does not look like the stassh-rust workspace."
}

ensure_base_requirements() {
  need_command cargo "Rust Cargo"
  need_command rustup "rustup"
  need_command rustc "rustc"
  need_command ssh "OpenSSH client"
  need_command ssh-keygen "OpenSSH ssh-keygen"
}

ensure_rust_target() {
  local target="$1"

  [[ -n "$target" ]] || return 0

  if rust_target_installed "$target"; then
    info "Rust target already installed: $target"
    return 0
  fi

  info "Installing missing Rust target: $target"
  rustup target add "$target" || return 1
}

ensure_debian_package() {
  local package="$1"

  [[ -n "$package" ]] || return 0

  if ! is_debian_like; then
    warn "missing linker package '$package'. Automatic package install is only supported on Debian/Ubuntu-like systems."
    return 1
  fi

  if package_installed "$package"; then
    info "Debian package already installed: $package"
    return 0
  fi

  if ! prompt_yes_no "Install missing Debian/Ubuntu package '$package' with sudo apt-get?" "y"; then
    warn "cannot build this target without package '$package'."
    return 1
  fi

  sudo apt-get update || return 1
  sudo apt-get install -y "$package" || return 1
}

ensure_linker() {
  local linker="$1"
  local package="$2"

  [[ -n "$linker" ]] || return 0

  if command -v "$linker" >/dev/null 2>&1; then
    info "Linker already available: $linker"
    return 0
  fi

  ensure_debian_package "$package" || return 1

  if ! command -v "$linker" >/dev/null 2>&1; then
    warn "package '$package' was installed, but linker '$linker' is still not available in PATH."
    return 1
  fi
}

run_build() {
  local label="$1"
  local target="$2"
  local linker_env_name="$3"
  local linker="$4"
  local package="$5"
  local -a cargo_args=(build -p stassh -p stassh-tui --release)
  local output_dir

  info
  info "Building: $label"

  ensure_rust_target "$target" || return 1
  ensure_linker "$linker" "$package" || return 1

  if [[ -n "$target" ]]; then
    cargo_args+=(--target "$target")
  fi

  if [[ -n "$linker_env_name" ]]; then
    env "$linker_env_name=$linker" RUSTFLAGS="$RUSTFLAGS_RELEASE" cargo "${cargo_args[@]}" || return 1
  else
    env RUSTFLAGS="$RUSTFLAGS_RELEASE" cargo "${cargo_args[@]}" || return 1
  fi

  output_dir="$(target_output_dir "$target")"
  for binary in "${BINARIES[@]}"; do
    if [[ ! -x "$output_dir/$binary" ]]; then
      warn "expected executable was not created: $output_dir/$binary"
      return 1
    fi
  done

  SUCCEEDED_TARGETS+=("$label|$target")
  info
  info "Build complete:"
  binary_paths_for_target "$target" | sed 's/^/  /'

  offer_install "$target"
}

build_target() {
  local key="$1"

  case "$key" in
    native)
      run_build "Native release for this machine" "" "" "" ""
      ;;
    x86_64_musl)
      run_build "Portable x86_64 Linux musl" "x86_64-unknown-linux-musl" "" "" ""
      ;;
    i686_musl)
      run_build "Portable i686 Linux musl for old x86 systems" "i686-unknown-linux-musl" "" "" ""
      ;;
    x86_64_gnu)
      run_build "x86_64 Linux GNU" "x86_64-unknown-linux-gnu" "" "" ""
      ;;
    i686_gnu)
      run_build \
        "i686 Linux GNU" \
        "i686-unknown-linux-gnu" \
        "CARGO_TARGET_I686_UNKNOWN_LINUX_GNU_LINKER" \
        "i686-linux-gnu-gcc" \
        "gcc-i686-linux-gnu"
      ;;
    armv6_gnueabihf)
      run_build \
        "ARMv6 Linux GNU hard-float" \
        "arm-unknown-linux-gnueabihf" \
        "CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER" \
        "arm-linux-gnueabihf-gcc" \
        "gcc-arm-linux-gnueabihf"
      ;;
    armv7_gnueabihf)
      run_build \
        "ARMv7 Linux GNU hard-float" \
        "armv7-unknown-linux-gnueabihf" \
        "CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER" \
        "arm-linux-gnueabihf-gcc" \
        "gcc-arm-linux-gnueabihf"
      ;;
    *)
      die "unknown target key: $key"
      ;;
  esac
}

offer_install() {
  local target="$1"
  local output_dir

  output_dir="$(target_output_dir "$target")"

  if ! prompt_yes_no "Install stassh and stassh-tui into $INSTALL_DIR?" "n"; then
    return 0
  fi

  sudo install -m 0755 "$output_dir/stassh" "$INSTALL_DIR/stassh"
  sudo install -m 0755 "$output_dir/stassh-tui" "$INSTALL_DIR/stassh-tui"

  info "Installed:"
  info "  $INSTALL_DIR/stassh"
  info "  $INSTALL_DIR/stassh-tui"

  if command -v stassh >/dev/null 2>&1 && command -v stassh-tui >/dev/null 2>&1; then
    info
    info "PATH checks:"
    stassh --help >/dev/null
    info "  stassh --help: ok"
    stassh-tui --help >/dev/null
    info "  stassh-tui --help: ok"
  else
    warn "$INSTALL_DIR is not currently in PATH, or shell command lookup has not refreshed."
  fi
}

print_menu() {
  cat <<'EOF'
Build target menu
  1) Native release for this machine
     -> target/release/stassh
     -> target/release/stassh-tui

  2) Portable x86_64 Linux musl
     -> target/x86_64-unknown-linux-musl/release/stassh
     -> target/x86_64-unknown-linux-musl/release/stassh-tui

  3) Portable i686 Linux musl for old x86 systems
     -> target/i686-unknown-linux-musl/release/stassh
     -> target/i686-unknown-linux-musl/release/stassh-tui

  4) x86_64 Linux GNU
     -> target/x86_64-unknown-linux-gnu/release/stassh
     -> target/x86_64-unknown-linux-gnu/release/stassh-tui

  5) i686 Linux GNU
     Needs linker: i686-linux-gnu-gcc
     Debian/Ubuntu package: gcc-i686-linux-gnu

  6) ARMv6 Linux GNU hard-float
     Needs linker: arm-linux-gnueabihf-gcc
     Debian/Ubuntu package: gcc-arm-linux-gnueabihf

  7) ARMv7 Linux GNU hard-float
     Needs linker: arm-linux-gnueabihf-gcc
     Debian/Ubuntu package: gcc-arm-linux-gnueabihf

  8) Build all targets above

  q) Quit
EOF
}

record_failure() {
  local label="$1"
  local status="$2"
  FAILED_TARGETS+=("$label (exit $status)")
}

build_target_with_summary() {
  local key="$1"
  local label="$2"

  if build_target "$key"; then
    return 0
  fi

  local status=$?
  record_failure "$label" "$status"
  warn "Build failed for $label."
  return "$status"
}

build_all_targets() {
  local -a keys=(
    native
    x86_64_musl
    i686_musl
    x86_64_gnu
    i686_gnu
    armv6_gnueabihf
    armv7_gnueabihf
  )
  local -a labels=(
    "Native release for this machine"
    "Portable x86_64 Linux musl"
    "Portable i686 Linux musl for old x86 systems"
    "x86_64 Linux GNU"
    "i686 Linux GNU"
    "ARMv6 Linux GNU hard-float"
    "ARMv7 Linux GNU hard-float"
  )
  local index

  for index in "${!keys[@]}"; do
    build_target_with_summary "${keys[$index]}" "${labels[$index]}" || true
  done
}

print_summary() {
  info
  info "Summary"

  if ((${#SUCCEEDED_TARGETS[@]})); then
    info "  Succeeded:"
    local item label target
    for item in "${SUCCEEDED_TARGETS[@]}"; do
      label="${item%%|*}"
      target="${item#*|}"
      info "    $label"
      binary_paths_for_target "$target" | sed 's/^/      /'
    done
  else
    info "  Succeeded: none"
  fi

  if ((${#FAILED_TARGETS[@]})); then
    info "  Failed:"
    printf '    %s\n' "${FAILED_TARGETS[@]}"
  fi
}

main() {
  local choice

  if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
  fi

  ensure_repo_root
  print_system_report
  ensure_base_requirements
  print_menu

  while true; do
    read -r -p "Choose a build target: " choice
    case "$choice" in
      1) build_target_with_summary native "Native release for this machine" || true; break ;;
      2) build_target_with_summary x86_64_musl "Portable x86_64 Linux musl" || true; break ;;
      3) build_target_with_summary i686_musl "Portable i686 Linux musl for old x86 systems" || true; break ;;
      4) build_target_with_summary x86_64_gnu "x86_64 Linux GNU" || true; break ;;
      5) build_target_with_summary i686_gnu "i686 Linux GNU" || true; break ;;
      6) build_target_with_summary armv6_gnueabihf "ARMv6 Linux GNU hard-float" || true; break ;;
      7) build_target_with_summary armv7_gnueabihf "ARMv7 Linux GNU hard-float" || true; break ;;
      8) build_all_targets; break ;;
      q|Q) info "No build selected."; exit 0 ;;
      *) info "Please choose 1-8, or q to quit." ;;
    esac
  done

  print_summary

  if ((${#FAILED_TARGETS[@]})); then
    exit 1
  fi
}

main "$@"
