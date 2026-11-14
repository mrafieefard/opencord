//! The plugin view stream for JavaScript (plugins milestone 4 plan, Task 1):
//! batches pulled in generations, as events are, and the ids of the views
//! this JavaScript start opened.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex as SyncMutex, PoisonError};

use opencord_core::api::types::ViewOutput;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{Mutex, Notify};

use crate::types::ViewOutputRecord;

/// The most batches one `next` returns.
pub(crate) const VIEW_BATCH_MAX: u32 = 256;

#[derive(Default)]
pub struct ViewPump {
    /// The stream of the newest attach, until a `next` takes it over.
    fresh: SyncMutex<Option<UnboundedReceiver<ViewOutput>>>,
    receiver: Mutex<Option<UnboundedReceiver<ViewOutput>>>,
    generation: AtomicU64,
    attached: Notify,
    /// Views opened and not closed since the last attach.
    open: SyncMutex<BTreeSet<String>>,
}

impl ViewPump {
    /// Starts a new generation reading `stream`: a call waiting under an
    /// older one returns an empty batch at once. Returns the generation and
    /// the views the earlier one left open, which the caller closes.
    pub fn attach(&self, stream: UnboundedReceiver<ViewOutput>) -> (u64, Vec<String>) {
        *self.fresh.lock().unwrap_or_else(PoisonError::into_inner) = Some(stream);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.attached.notify_waiters();
        let left = std::mem::take(&mut *self.open.lock().unwrap_or_else(PoisonError::into_inner));
        (generation, left.into_iter().collect())
    }

    /// Waits for at least one batch, then returns what is ready, up to
    /// `max`, in order. Empty only when a newer generation has attached, or
    /// the core has stopped.
    pub async fn next(&self, generation: u64, max: u32) -> Vec<ViewOutputRecord> {
        let max = max.clamp(1, VIEW_BATCH_MAX) as usize;
        // Registered before the generation is read, so an `attach` between
        // the two can't be missed.
        let superseded = self.attached.notified();
        tokio::pin!(superseded);
        superseded.as_mut().enable();
        if self.generation.load(Ordering::SeqCst) != generation {
            return Vec::new();
        }
        let mut receiver = tokio::select! {
            receiver = self.receiver.lock() => receiver,
            () = &mut superseded => return Vec::new(),
        };
        if let Some(fresh) = self
            .fresh
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            *receiver = Some(fresh);
        }
        if self.generation.load(Ordering::SeqCst) != generation {
            return Vec::new();
        }
        let Some(stream) = receiver.as_mut() else {
            return Vec::new();
        };
        let first = tokio::select! {
            output = stream.recv() => output,
            () = &mut superseded => return Vec::new(),
        };
        let Some(first) = first else {
            return Vec::new();
        };
        let mut batch = vec![record(first)];
        while batch.len() < max {
            let Ok(next) = stream.try_recv() else {
                break;
            };
            batch.push(record(next));
        }
        batch
    }

    pub fn opened(&self, view_id: &str) {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(view_id.to_owned());
    }

    pub fn closed(&self, view_id: &str) {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(view_id);
    }

    pub fn open_ids(&self) -> Vec<String> {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

fn record(output: ViewOutput) -> ViewOutputRecord {
    ViewOutputRecord {
        view_id: output.view_id,
        json: output.json,
    }
}
