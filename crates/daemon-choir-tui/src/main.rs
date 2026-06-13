use std::io::{self, Write};
use std::time::Duration;
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Status {
    version: String,
    build_commit: String,
    uptime_secs: u64,
    state: String,
    probes_loaded: usize,
    backends_active: usize,
    api_version: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct ProbeInfo {
    name: String,
    attach_point: String,
    loaded: bool,
    events_total: u64,
    events_lost: u64,
    last_event_us: u64,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct StateSnapshot {
    timestamp_us: u64,
    window_ms: u64,
    stale: bool,
    lost_events: u64,
    metrics: Metrics,
}

#[derive(Deserialize, Debug)]
struct Metrics {
    #[serde(rename = "cpu.sched.latency_p95")]
    cpu_sched_latency_p95: f64,
    #[serde(rename = "cpu.sched.context_rate")]
    cpu_sched_context_rate: f64,
    #[serde(rename = "mem.reclaim.pressure")]
    mem_reclaim_pressure: f64,
    #[serde(rename = "net.tx.bytes_norm")]
    net_tx_bytes_norm: f64,
    #[serde(rename = "net.rx.bytes_norm")]
    net_rx_bytes_norm: f64,
    #[serde(rename = "proc.exec.rate")]
    proc_exec_rate: f64,
}

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();
    let url = "http://127.0.0.1:9876";

    println!("==========================================================");
    println!("             DAEMON CHOIR TUI CLI CLIENT");
    println!("==========================================================");
    println!("Connecting to control API at {}...", url);

    // Let's run a check to see if the server is up
    match client.get(&format!("{}/v1/status", url)).send().await {
        Ok(_) => {
            println!("Connection established!");
        }
        Err(e) => {
            println!("Error: Could not connect to daemon-choir at {}.", url);
            println!("Is the conductor running? Start it first using:");
            println!("  cargo run --bin conductor");
            println!("\nDetails: {}", e);
            std::process::exit(1);
        }
    }

    // Let's display our main interactive options
    println!("\nCommands available:");
    println!("  1. View Real-time Telemetry Dashboard (ASCII map)");
    println!("  2. View Active Config & Voice Mapping");
    println!("  3. Trigger Hot Reload (SIGHUP equivalent)");
    println!("  4. Trigger Privacy Wipe (truncate all data)");
    println!("  5. Shutdown Daemon gracefully");
    println!("  6. Exit TUI");

    loop {
        print!("\nEnter command number: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let cmd = input.trim();

        match cmd {
            "1" => {
                run_dashboard(&client, url).await;
            }
            "2" => {
                view_config(&client, url).await;
            }
            "3" => {
                trigger_reload(&client, url).await;
            }
            "4" => {
                trigger_wipe(&client, url).await;
            }
            "5" => {
                trigger_shutdown(&client, url).await;
                break;
            }
            "6" => {
                println!("Exiting TUI. Goodbye!");
                break;
            }
            _ => {
                println!("Invalid selection, please select a number between 1 and 6.");
            }
        }
    }
}

async fn run_dashboard(client: &reqwest::Client, url: &str) {
    println!("\n--- Entering Dashboard Mode. Press Ctrl+C to return to main menu ---");
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    
    loop {
        interval.tick().await;

        // Fetch daemon status safely to satisfy BUG-05
        let status_res = match client.get(&format!("{}/v1/status", url)).send().await {
            Ok(res) => match res.error_for_status() {
                Ok(r) => r.json::<Status>().await.ok(),
                Err(_) => None,
            },
            Err(_) => None,
        };

        // Fetch probes safely to satisfy BUG-05
        let probes_res = match client.get(&format!("{}/v1/probes", url)).send().await {
            Ok(res) => match res.error_for_status() {
                Ok(r) => r.json::<Vec<ProbeInfo>>().await.ok(),
                Err(_) => None,
            },
            Err(_) => None,
        };

        // Fetch metrics state safely to satisfy BUG-05
        let state_res = match client.get(&format!("{}/v1/state", url)).send().await {
            Ok(res) => match res.error_for_status() {
                Ok(r) => r.json::<StateSnapshot>().await.ok(),
                Err(_) => None,
            },
            Err(_) => None,
        };

        if let (Some(s), Some(p), Some(st)) = (status_res, probes_res, state_res) {
            // Clear terminal screen using ANSI escape codes
            print!("\x1B[2J\x1B[1;1H");

            println!("┌────────────────────────────────────────────────────────┐");
            println!("│ DAEMON CHOIR (Conductor v{}) STATUS               │", s.version);
            println!("├────────────────────────────────────────────────────────┤");
            println!("│ State: {:<10} | Uptime: {:<8}s | Backends: {:<5} │", s.state, s.uptime_secs, s.backends_active);
            println!("├────────────────────────────────────────────────────────┤");
            println!("│ ACTIVE KERNEL TELEMETRY METRICS (Normalized 0.0 - 1.0) │");
            println!("├────────────────────────────────────────────────────────┤");
            
            draw_bar("CPU Latency (p95) ", st.metrics.cpu_sched_latency_p95);
            draw_bar("CPU Context Switches", st.metrics.cpu_sched_context_rate);
            draw_bar("Memory Pressure     ", st.metrics.mem_reclaim_pressure);
            draw_bar("Network Tx Rate     ", st.metrics.net_tx_bytes_norm);
            draw_bar("Network Rx Rate     ", st.metrics.net_rx_bytes_norm);
            draw_bar("Process Exec Rate   ", st.metrics.proc_exec_rate);

            println!("├────────────────────────────────────────────────────────┤");
            println!("│ EBPF PROBES / TELEMETRY RING BUFFER METRIC COUNTERS    │");
            println!("├────────────────────────────────────────────────────────┤");
            for probe in p {
                println!(
                    "│ {:<14} | Loaded: {:<5} | Events: {:<6} | Lost: {:<3} │",
                    probe.name,
                    probe.loaded,
                    probe.events_total,
                    probe.events_lost
                );
            }
            println!("└────────────────────────────────────────────────────────┘");
            println!("  Press Ctrl+C to go back to CLI menu.");
        } else {
            println!("\n⚠️ Connection lost or error communicating with daemon-choir. Exiting dashboard.");
            break;
        }
    }
}

fn draw_bar(label: &str, val: f64) {
    let width = 30;
    let filled = (val * width as f64).round() as usize;
    let bar = format!(
        "{}{}",
        "#".repeat(filled),
        ".".repeat(width - filled)
    );
    println!("│ {:<20} | [{}] ({:.2}) │", label, bar, val);
}

async fn view_config(client: &reqwest::Client, url: &str) {
    println!("\nFetching voice mappings from daemon...");
    match client.get(&format!("{}/v1/voices", url)).send().await {
        Ok(res) => {
            if let Ok(text) = res.text().await {
                println!("\nActive Voice Mapping Map:");
                println!("{}", text);
            }
        }
        Err(e) => println!("Error getting voice maps: {}", e),
    }
}

async fn trigger_reload(client: &reqwest::Client, url: &str) {
    println!("\nSending hot reload request to daemon...");
    match client.post(&format!("{}/v1/config/reload", url)).send().await {
        Ok(res) => {
            if res.status().is_success() {
                println!("✅ Hot reload trigger ACCEPTED! Mapping rules updated.");
            } else {
                println!("❌ Error: Received status code {}", res.status());
            }
        }
        Err(e) => println!("Error during hot reload: {}", e),
    }
}

async fn trigger_wipe(client: &reqwest::Client, url: &str) {
    println!("\n⚠️ WARNING: This will delete all SQLite records and session logs.");
    print!("Are you sure? (y/N): ");
    io::stdout().flush().unwrap();

    let mut confirm = String::new();
    io::stdin().read_line(&mut confirm).unwrap();
    if confirm.trim().to_lowercase() == "y" {
        match client
            .post(&format!("{}/v1/privacy/wipe", url))
            .header("X-Daemon-Choir", "wipe") // BUG-08 protect route
            .send()
            .await
        {
            Ok(res) => {
                if res.status().is_success() {
                    println!("✅ Privacy wipe completed successfully.");
                } else {
                    println!("❌ Error: Received status code {}", res.status());
                }
            }
            Err(e) => println!("Error sending wipe request: {}", e),
        }
    } else {
        println!("Aborted.");
    }
}

async fn trigger_shutdown(client: &reqwest::Client, url: &str) {
    println!("\nSending graceful shutdown request...");
    match client
        .post(&format!("{}/v1/shutdown", url))
        .header("X-Daemon-Choir", "shutdown")
        .send()
        .await
    {
        Ok(res) => {
            if res.status().is_success() {
                println!("✅ Shutdown command sent. Conductor is shutting down.");
            } else {
                println!("❌ Failed to shut down. Status: {}", res.status());
            }
        }
        Err(e) => println!("Error during shutdown: {}", e),
    }
}
