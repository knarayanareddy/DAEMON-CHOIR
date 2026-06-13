#!/usr/bin/env python3
"""
==========================================================================
                     DAEMON CHOIR - REST API Python Client
==========================================================================
This script connects to the local Conductor REST API at 127.0.0.1:9876 and
polls the live telemetry state. It parses the JSON metrics and draws a
real-time ASCII dashboard in the console.

Runs out-of-the-box using the standard Python library (no external packages).

Usage:
  python3 telemetry_client.py
==========================================================================
"""

import os
import sys
import time
import json
import urllib.request
import urllib.error

API_URL = "http://127.0.0.1:9876"
INTERVAL_SECS = 0.5

def fetch_json(endpoint):
    url = f"{API_URL}{endpoint}"
    try:
        with urllib.request.urlopen(url, timeout=2) as response:
            if response.status == 200:
                return json.loads(response.read().decode("utf-8"))
    except urllib.error.URLError as e:
        # Cannot connect to API
        return None
    return None

def draw_bar(label, value, width=30):
    # Clamps value to 0.0 - 1.0 range
    val_clamped = max(0.0, min(1.0, float(value)))
    filled_len = int(round(val_clamped * width))
    bar = "#" * filled_len + "." * (width - filled_len)
    return f"{label:<22} | [{bar}] ({val_clamped:.2f})"

def main():
    print(f"Connecting to DAEMON CHOIR API at {API_URL}...")
    
    # Verify connection
    status = fetch_json("/v1/status")
    if not status:
        print(f"Error: Could not connect to daemon at {API_URL}.")
        print("Is the Conductor running? Start it using: cargo run --bin conductor")
        sys.exit(1)
        
    print(f"Connected to Conductor v{status.get('version')} (commit {status.get('build_commit')[:7]})")
    print("Entering live telemetry dashboard loop. Press Ctrl+C to exit.\n")
    time.sleep(1.5)

    try:
        while True:
            status = fetch_json("/v1/status")
            state = fetch_json("/v1/state")
            probes = fetch_json("/v1/probes")

            if not status or not state or not probes:
                print("\n⚠️ Connection lost. Retrying in 2 seconds...")
                time.sleep(2)
                continue

            # Clear terminal using ANSI codes
            os.system("clear" if os.name != "nt" else "cls")

            metrics = state.get("metrics", {})
            stale_str = "⚠️ STALE" if state.get("stale") else "🟢 ACTIVE"

            print("┌──────────────────────────────────────────────────────────┐")
            print(f"│ DAEMON CHOIR Live REST Telemetry Dashboard               │")
            print("├──────────────────────────────────────────────────────────┤")
            print(f"│ State: {status.get('state'):<10} | Uptime: {status.get('uptime_secs'):<6}s | Data Status: {stale_str:<8} │")
            print("├──────────────────────────────────────────────────────────┤")
            print("│ NORMALIZED TELEMETRY CHANNELS (0.0 - 1.0)                │")
            print("├──────────────────────────────────────────────────────────┤")
            print(f"│ {draw_bar('CPU Latency (p95)', metrics.get('cpu.sched.latency_p95', 0.0))} │")
            print(f"│ {draw_bar('CPU Context Switches', metrics.get('cpu.sched.context_rate', 0.0))} │")
            print(f"│ {draw_bar('Memory Reclaim Press', metrics.get('mem.reclaim.pressure', 0.0))} │")
            print(f"│ Network Transmit Rate", metrics.get("net.tx.bytes_norm", 0.0))
            print(f"│ {draw_bar('Network Transmit (Tx)', metrics.get('net.tx.bytes_norm', 0.0))} │")
            print(f"│ {draw_bar('Network Receive (Rx)', metrics.get('net.rx.bytes_norm', 0.0))} │")
            print(f"│ {draw_bar('Process Exec Rate', metrics.get('proc.exec.rate', 0.0))} │")
            print("├──────────────────────────────────────────────────────────┤")
            print("│ ACTIVE eBPF KERNEL PROBES                                │")
            print("├──────────────────────────────────────────────────────────┤")
            for probe in probes:
                name = probe.get("name")
                loaded = "Loaded" if probe.get("loaded") else "Offline"
                events = probe.get("events_total", 0)
                lost = probe.get("events_lost", 0)
                print(f"│ {name:<15} | {loaded:<7} | Events: {events:<6} | Lost: {lost:<3} │")
            print("└──────────────────────────────────────────────────────────┘")
            print(" Press Ctrl+C to return to terminal prompt.")

            time.sleep(INTERVAL_SECS)

    except KeyboardInterrupt:
        print("\nExiting dashboard client. Goodbye!")

if __name__ == "__main__":
    main()
