#!/usr/bin/env bash

# Build and install this checkout, then create desktop and autostart entries.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname -- "$SCRIPT_DIR")"
CARGO_DIR="${CARGO_HOME:-${HOME}/.cargo}"
DATA_DIR="${XDG_DATA_HOME:-${HOME}/.local/share}"
CONFIG_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}"
DESKTOP_ENTRY="${DATA_DIR}/applications/solar-screen-brightness.desktop"
AUTOSTART_DIR="${CONFIG_DIR}/autostart"
AUTOSTART_ENTRY="${AUTOSTART_DIR}/solar-screen-brightness.desktop"
SSB_BINARY="${CARGO_DIR}/bin/ssb"
SSB_DATA_DIR="${DATA_DIR}/solar-screen-brightness"
ICON_PATH="${SSB_DATA_DIR}/icon.png"
SSB_CONFIG_DIR="${CONFIG_DIR}/solar-screen-brightness"

install_package_if_not_exists() {
  if dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q '^install ok installed$'; then
    echo "Package $1 is already installed"
  else
    echo "Installing $1"
    sudo apt-get install -y "$1"
  fi
}

refresh_desktop() {
  if command -v xdg-desktop-menu >/dev/null; then
    xdg-desktop-menu forceupdate
  fi
}

# All Desktop Entry strings are unescaped before Exec's argument parsing.
desktop_string_value() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//$'\n'/\\n}"
  value="${value//$'\r'/\\r}"
  value="${value//$'\t'/\\t}"
  printf '%s' "$value"
}

# Quote the executable for Exec, then escape that quoted Desktop Entry value.
desktop_exec_path() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//\$/\\\$}"
  value="${value//\`/\\\`}"
  value="${value//%/%%}"
  desktop_string_value "$value"
}

install() {
  if ! command -v cargo >/dev/null; then
    echo "Rust Cargo is required. Install Rust from https://rustup.rs and retry."
    return 1
  fi
  for package in build-essential pkg-config libssl-dev libudev-dev; do
    install_package_if_not_exists "$package"
  done

  echo "Installing Sunshift Brightness from ${REPO_DIR}"
  cargo install --locked --path "$REPO_DIR" --root "$CARGO_DIR"
  mkdir -p "${SSB_DATA_DIR}" "$(dirname -- "$DESKTOP_ENTRY")" "$AUTOSTART_DIR"
  cp -- "${REPO_DIR}/assets/icon-256.png" "$ICON_PATH"
  local exec_path icon_path exec_command
  exec_path="$(desktop_exec_path "$SSB_BINARY")"
  icon_path="$(desktop_string_value "$ICON_PATH")"
  exec_command="\"${exec_path}\""
  # GLib checks the executable before expanding %% to a literal percent.
  # Put such a path in env's argument instead of the executable field.
  if [[ "$SSB_BINARY" == *%* ]]; then
    exec_command="/usr/bin/env ${exec_command}"
  fi
  cat > "$DESKTOP_ENTRY" <<EOT
[Desktop Entry]
Type=Application
Name=Solar Screen Brightness
Exec=${exec_command}
Icon=${icon_path}
Terminal=false
EOT
  cat > "$AUTOSTART_ENTRY" <<EOT
[Desktop Entry]
Type=Application
Name=Solar Screen Brightness
Exec=${exec_command} --minimised
Icon=${icon_path}
Terminal=false
EOT
  refresh_desktop
  echo "Successfully installed $("${CARGO_DIR}/bin/ssb-cli" --version)"
}

uninstall() {
  rm -f -- "$DESKTOP_ENTRY" "$AUTOSTART_ENTRY"
  refresh_desktop
  cargo uninstall --root "$CARGO_DIR" solar-screen-brightness
  echo "Deleting configuration and application data"
  rm -rf -- "$SSB_CONFIG_DIR" "$SSB_DATA_DIR"
}

case "${1:-}" in
  install) install ;;
  uninstall) uninstall ;;
  *) echo "Usage: ./linux/ubuntu_install.sh [install|uninstall]"; exit 2 ;;
esac
