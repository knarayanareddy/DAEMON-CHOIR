#!/usr/bin/env bash
set -e

echo "=========================================================="
echo "          DAEMON CHOIR - Uninstallation Script"
echo "=========================================================="

echo "1. Stopping and disabling systemd user service..."
systemctl --user stop daemon-choir.service || true
systemctl --user disable daemon-choir.service || true

# Remove systemd service file
USER_SYSTEMD_DIR="$HOME/.config/systemd/user"
rm -f "$USER_SYSTEMD_DIR/daemon-choir.service"
systemctl --user daemon-reload

echo "2. Removing binaries from /usr/local/bin..."
sudo rm -f /usr/local/bin/daemon-choir
sudo rm -f /usr/local/bin/daemon-choir-tui

echo "3. Removing configuration directories..."
rm -rf "$HOME/.config/daemon-choir"

echo "4. Purging runtime and data files..."
# Delete on-disk SQLite database as per §7.4
rm -rf "$HOME/.local/share/daemon-choir"

echo "=========================================================="
echo "✅ DAEMON CHOIR has been successfully uninstalled."
echo "=========================================================="
