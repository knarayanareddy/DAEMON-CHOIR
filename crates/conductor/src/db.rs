use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use rusqlite::Connection;
use tracing::{info, error};
use daemon_choir_common::events::RawEvent;
use daemon_choir_common::metrics::StateSnapshot;

pub struct Database {
    conn: Mutex<Option<Connection>>,
}

impl Database {
    pub fn new(path: Option<PathBuf>) -> Self {
        let db_path = path.unwrap_or_else(|| {
            let base_dir = std::env::var("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
                    let mut p = PathBuf::from(home);
                    p.push(".local");
                    p.push("share");
                    p
                });
            let mut p = base_dir;
            p.push("daemon-choir");
            p
        });

        if let Err(e) = fs::create_dir_all(&db_path) {
            error!("Failed to create directory for database: {}", e);
            return Self { conn: Mutex::new(None) };
        }

        let mut file_path = db_path.clone();
        file_path.push("state.db");

        info!("Opening SQLite database at {:?}", file_path);

        // Attempt to open the connection
        let conn = match Connection::open(&file_path) {
            Ok(c) => {
                // Apply mode 0600 on Unix platforms as per §7.4
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(metadata) = fs::metadata(&file_path) {
                        let mut perms = metadata.permissions();
                        perms.set_mode(0o600);
                        if let Err(e) = fs::set_permissions(&file_path, perms) {
                            error!("Failed to set permissions 0600 on database file: {}", e);
                        }
                    }
                }
                Some(c)
            }
            Err(e) => {
                error!("Failed to open SQLite database: {}. Running in-memory/disabled mode.", e);
                None
            }
        };

        let mut db = Self { conn: Mutex::new(conn) };
        db.init_migrations();
        db
    }

    fn init_migrations(&mut self) {
        let conn_lock = self.conn.get_mut().unwrap();
        let conn = match conn_lock {
            Some(c) => c,
            None => return,
        };

        info!("Applying SQLite migrations...");
        let migration_queries = [
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                probe TEXT,
                timestamp_ns INTEGER,
                payload TEXT
            );",
            "CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp_us INTEGER,
                cpu_latency REAL,
                cpu_rate REAL,
                mem_pressure REAL,
                net_tx REAL,
                net_rx REAL,
                proc_exec REAL
            );"
        ];

        for query in &migration_queries {
            if let Err(e) = conn.execute(query, []) {
                error!("Database migration failed: {}. Disabling persistence.", e);
                *conn_lock = None;
                return;
            }
        }
        info!("SQLite migrations complete.");
    }

    pub fn insert_event(&self, event: &RawEvent) {
        let mut conn_lock = self.conn.lock().unwrap();
        let conn = match &mut *conn_lock {
            Some(c) => c,
            None => return,
        };

        let payload_json = serde_json::to_string(&event.payload).unwrap_or_default();
        let probe_str = event.probe.to_string();

        if let Err(e) = conn.execute(
            "INSERT INTO events (probe, timestamp_ns, payload) VALUES (?1, ?2, ?3)",
            rusqlite::params![probe_str, event.timestamp_ns, payload_json],
        ) {
            error!("SQLite write failure (insert_event): {}. Disabling database persistence.", e);
            // Disable database persistence on first write failure to satisfy §7.3
            *conn_lock = None;
        }
    }

    pub fn insert_snapshot(&self, snapshot: &StateSnapshot) {
        let mut conn_lock = self.conn.lock().unwrap();
        let conn = match &mut *conn_lock {
            Some(c) => c,
            None => return,
        };

        if let Err(e) = conn.execute(
            "INSERT INTO snapshots (timestamp_us, cpu_latency, cpu_rate, mem_pressure, net_tx, net_rx, proc_exec)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                snapshot.timestamp_us,
                snapshot.metrics.cpu_sched_latency_p95,
                snapshot.metrics.cpu_sched_context_rate,
                snapshot.metrics.mem_reclaim_pressure,
                snapshot.metrics.net_tx_bytes_norm,
                snapshot.metrics.net_rx_bytes_norm,
                snapshot.metrics.proc_exec_rate,
            ],
        ) {
            error!("SQLite write failure (insert_snapshot): {}. Disabling database persistence.", e);
            // Disable database persistence on first write failure to satisfy §7.3
            *conn_lock = None;
        }
    }

    pub fn purge_older_than(&self, retention: Duration) {
        let mut conn_lock = self.conn.lock().unwrap();
        let conn = match &mut *conn_lock {
            Some(c) => c,
            None => return,
        };

        let now_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_micros() as u64;

        let cutoff_us = now_us.saturating_sub(retention.as_micros() as u64);
        let cutoff_ns = cutoff_us.saturating_mul(1000);

        let mut has_failed = false;
        if let Err(e) = conn.execute("DELETE FROM events WHERE timestamp_ns < ?1", [cutoff_ns]) {
            error!("Failed to purge old raw events: {}. Disabling database persistence.", e);
            has_failed = true;
        }
        if !has_failed {
            if let Err(e) = conn.execute("DELETE FROM snapshots WHERE timestamp_us < ?1", [cutoff_us]) {
                error!("Failed to purge old state snapshots: {}. Disabling database persistence.", e);
                has_failed = true;
            }
        }

        if has_failed {
            *conn_lock = None;
        } else {
            info!("Pruned historical metrics older than 24 hours from local SQLite store.");
        }
    }

    pub fn wipe(&self) -> Result<(), rusqlite::Error> {
        let mut conn_lock = self.conn.lock().unwrap();
        let conn = match &mut *conn_lock {
            Some(c) => c,
            None => return Ok(()),
        };

        if let Err(e) = conn.execute("DELETE FROM events", []) {
            error!("Database wipe failed on events: {}. Disabling database persistence.", e);
            *conn_lock = None;
            return Err(e);
        }
        if let Err(e) = conn.execute("DELETE FROM snapshots", []) {
            error!("Database wipe failed on snapshots: {}. Disabling database persistence.", e);
            *conn_lock = None;
            return Err(e);
        }
        Ok(())
    }
}
