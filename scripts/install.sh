#!/usr/bin/env bash
set -e

echo "=========================================================="
echo "          DAEMON CHOIR - Installation Script"
echo "=========================================================="

# Ensure we are compiling on a supported platform
if [[ "$OSTYPE" != "linux-gnu"* ]]; then
    echo "Error: DAEMON CHOIR only supports Linux platforms."
    exit 1
fi

echo "1. Building DAEMON CHOIR binaries..."
cargo build --release

echo "2. Installing binaries to /usr/local/bin..."
sudo cp target/release/conductor /usr/local/bin/daemon-choir
sudo cp target/release/daemon-choir-tui /usr/local/bin/daemon-choir-tui

echo "3. Granting kernel capabilities (CAP_BPF, CAP_PERFMON)..."
# Allows unprivileged execution of eBPF programs as specified in §9.3
if sudo setcap 'cap_bpf,cap_perfmon=ep' /usr/local/bin/daemon-choir; then
    echo "✅ Kernel capabilities applied successfully."
else
    echo "⚠️ Warning: Failed to apply setcap. Running daemon might require root privileges."
fi

echo "4. Installing systemd service..."
# Install as user service as per §13.2
USER_SYSTEMD_DIR="$HOME/.config/systemd/user"
mkdir -p "$USER_SYSTEMD_DIR"
cp systemd/daemon-choir.service "$USER_SYSTEMD_DIR/daemon-choir.service"

echo "5. Creating default configuration files..."
CONFIG_DIR="$HOME/.config/daemon-choir"
mkdir -p "$CONFIG_DIR"
if [ ! -f "$CONFIG_DIR/daemon-choir.config.toml" ]; then
    cat << 'EOF' > "$CONFIG_DIR/daemon-choir.config.toml"
[daemon]
window_ms = 50
log_level = "info"
log_format = "pretty"
control_api_port = 9876
simulate = true

[probes]
enabled = ["cpu_sched", "mem_pressure", "net_io", "proc_lifecycle"]

[backends]
[[backends.osc]]
name = "supercollider"
address = "127.0.0.1:57120"

[[mapping]]
source_metric = "cpu.sched.latency_p95"
osc_address = "/choir/voice/0/frequency"
transform = "exponential"
input_range = [0.0, 1.0]
output_range = [220.0, 880.0]

[meta]
schema_version = "1"
EOF
    echo "✅ Default config file created at $CONFIG_DIR/daemon-choir.config.toml"
fi

echo "6. Enabling and starting systemd user service..."
systemctl --user daemon-reload
systemctl --user enable daemon-choir.service
systemctl --user restart daemon-choir.service

echo "=========================================================="
echo "🎉 DAEMON CHOIR successfully installed and running!"
echo "Check daemon status with: systemctl --user status daemon-choir"
echo "Launch TUI Client with: daemon-choir-tui"
echo "=========================================================="
