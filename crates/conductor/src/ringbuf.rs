use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};
use daemon_choir_common::events::RawEvent;
use crate::db::Database;

pub struct RingbufConsumer {
    rx: mpsc::Receiver<RawEvent>,
    aggregator_tx: mpsc::Sender<RawEvent>,
    db: Arc<Database>,
}

impl RingbufConsumer {
    pub fn new(rx: mpsc::Receiver<RawEvent>, aggregator_tx: mpsc::Sender<RawEvent>, db: Arc<Database>) -> Self {
        Self { rx, aggregator_tx, db }
    }

    pub async fn run(mut self) {
        info!("Starting Ring Buffer Consumer task...");
        while let Some(event) = self.rx.recv().await {
            // Persist the raw event to SQLite asynchronously to satisfy §7.4 without blocking the audio path
            let db_clone = self.db.clone();
            let event_clone = event.clone();
            tokio::spawn(async move {
                db_clone.insert_event(&event_clone);
            });

            // Forward straight to the aggregator
            if let Err(e) = self.aggregator_tx.try_send(event) {
                match e {
                    mpsc::error::TrySendError::Full(_) => {
                        warn!("Aggregator queue is full! Backpressure triggered, event dropped.");
                    }
                    mpsc::error::TrySendError::Closed(_) => {
                        info!("Aggregator queue closed, exiting RingbufConsumer.");
                        break;
                    }
                }
            }
        }
    }
}
