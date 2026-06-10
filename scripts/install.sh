#!/usr/bin/env bash
set -euo pipefail

# Run this on the Linux target after scp'ing the contents of a release/<target>/
# directory into a temp dir. It installs the binary into /usr/local/bin, the
# config and secret into /etc/dyndns, and a systemd system timer that runs dyndns
# every 300 seconds. Must be run as root (e.g. via sudo).

log_info() {
    echo -e "\033[1;32m[INFO]\033[0m $1"
}

log_error() {
    echo -e "\033[1;31m[ERROR]\033[0m $1"
}

if [[ $EUID -ne 0 ]]; then
    log_error "This installer must run as root. Re-run with: sudo bash $0"
    exit 1
fi

# Operate relative to this script's directory (the scp'ed temp dir)
cd "$(dirname "$0")"

BIN_NAME="dyndns"
BIN_DEST="/usr/local/bin/dyndns"
CONF_DIR="/etc/dyndns"
UNIT_DIR="/etc/systemd/system"

for f in "$BIN_NAME" config.toml .env; do
    if [[ ! -f "$f" ]]; then
        log_error "Required file '$f' not found in $(pwd). Copy the release directory contents here first."
        exit 1
    fi
done

log_info "Installing binary to $BIN_DEST ..."
install -m 755 "$BIN_NAME" "$BIN_DEST"

log_info "Installing config and secret to $CONF_DIR ..."
install -d -m 755 "$CONF_DIR"
install -m 644 config.toml "$CONF_DIR/config.toml"
install -m 600 .env "$CONF_DIR/.env"
[[ -f README.md ]] && install -m 644 README.md "$CONF_DIR/README.md"

# Pin the cache to the system path (managed by the unit's CacheDirectory below),
# overriding whatever cache_file the shipped config carried.
CACHE_PATH="/var/cache/dyndns/dyndns.cache"
if grep -qE '^[[:space:]]*cache_file[[:space:]]*=' "$CONF_DIR/config.toml"; then
    sed -i "s|^[[:space:]]*cache_file[[:space:]]*=.*|cache_file = \"$CACHE_PATH\"|" "$CONF_DIR/config.toml"
else
    echo "cache_file = \"$CACHE_PATH\"" >> "$CONF_DIR/config.toml"
fi
log_info "Set cache_file to $CACHE_PATH in $CONF_DIR/config.toml"

log_info "Installing systemd units to $UNIT_DIR ..."

# CacheDirectory creates and owns /var/cache/dyndns; stdout/stderr lands in the journal.
cat > "$UNIT_DIR/dyndns.service" <<'EOF'
[Unit]
Description=Dynamic DNS Update Client
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
CacheDirectory=dyndns
ExecStart=/usr/local/bin/dyndns --config /etc/dyndns/config.toml
EOF

cat > "$UNIT_DIR/dyndns.timer" <<'EOF'
[Unit]
Description=Run dyndns every 300 seconds

[Timer]
OnBootSec=60
OnUnitActiveSec=300
Unit=dyndns.service

[Install]
WantedBy=timers.target
EOF

log_info "Enabling and starting the timer ..."
systemctl daemon-reload
systemctl enable --now dyndns.timer

# Run once now: performs an immediate update and anchors OnUnitActiveSec so the
# timer is scheduled even when installing on an already-booted system.
log_info "Running an initial update ..."
if ! systemctl start dyndns.service; then
    log_error "Initial run failed. Check: journalctl -u dyndns.service"
fi

log_info "Done. Timer status:"
systemctl list-timers dyndns.timer --no-pager || true

cat <<'EOF'

dyndns is installed and scheduled to run every 300 seconds.

  - Check logs:    journalctl -u dyndns.service
  - Run once now:  systemctl start dyndns.service
  - Stop/disable:  systemctl disable --now dyndns.timer

  - Read logs without sudo (optional):
      sudo usermod -aG systemd-journal <user>   # re-login to take effect
EOF
