//! A plugin's waiting events (plan Task 7). Pushing never blocks: past
//! [`EVENT_QUEUE`] events the oldest are dropped and counted, and the
//! runtime logs the count.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use opencord_plugins::limits::EVENT_QUEUE;
use tokio::sync::Notify;

use super::PluginEvent;

pub struct EventQueue {
    inner: Mutex<Waiting>,
    ready: Notify,
    capacity: usize,
}

#[derive(Default)]
struct Waiting {
    events: VecDeque<PluginEvent>,
    dropped: u64,
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::with_capacity(EVENT_QUEUE)
    }
}

impl EventQueue {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Waiting::default()),
            ready: Notify::new(),
            capacity,
        }
    }

    /// Adds an event, dropping the oldest when full. A dropped event's
    /// reply sender goes with it, so whoever waits hears at once.
    pub fn push(&self, event: PluginEvent) {
        {
            let mut inner = self.lock();
            if inner.events.len() >= self.capacity {
                inner.events.pop_front();
                inner.dropped += 1;
            }
            inner.events.push_back(event);
        }
        self.ready.notify_one();
    }

    pub fn try_pop(&self) -> Option<PluginEvent> {
        self.lock().events.pop_front()
    }

    /// The next event, waiting for one.
    pub async fn pop(&self) -> PluginEvent {
        loop {
            if let Some(event) = self.try_pop() {
                return event;
            }
            // A push between `try_pop` and here leaves a permit, so this
            // can't miss it.
            self.ready.notified().await;
        }
    }

    /// Removes and returns the waiting events named `name`, in order.
    pub fn take_named(&self, name: &str) -> Vec<PluginEvent> {
        let mut inner = self.lock();
        let mut taken = Vec::new();
        for event in std::mem::take(&mut inner.events) {
            if event.name == name {
                taken.push(event);
            } else {
                inner.events.push_back(event);
            }
        }
        taken
    }

    /// How many events were dropped since the last call.
    pub fn take_dropped(&self) -> u64 {
        std::mem::take(&mut self.lock().dropped)
    }

    /// Drops every waiting event: the plugin stopped.
    pub fn clear(&self) {
        let events = std::mem::take(&mut self.lock().events);
        // Dropped outside the lock: each may wake a waiter.
        drop(events);
    }

    fn lock(&self) -> MutexGuard<'_, Waiting> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: String) -> PluginEvent {
        PluginEvent {
            name,
            payload: serde_json::Value::Null,
            reply: None,
            command: None,
        }
    }

    // Catches: dropping the newest (the plugin would never see what is
    // happening now), or blocking the server's fan-out.
    #[test]
    fn the_oldest_events_are_dropped_when_the_queue_is_full() {
        let queue = EventQueue::default();
        for index in 0..EVENT_QUEUE + 5 {
            queue.push(named(index.to_string()));
        }

        assert_eq!(queue.take_dropped(), 5);
        assert_eq!(queue.take_dropped(), 0);
        assert_eq!(queue.try_pop().unwrap().name, "5");
        let mut last = None;
        while let Some(event) = queue.try_pop() {
            last = Some(event.name);
        }
        assert_eq!(last.as_deref(), Some(&*(EVENT_QUEUE + 4).to_string()));
    }

    #[tokio::test]
    async fn pop_waits_for_a_push() {
        let queue = std::sync::Arc::new(EventQueue::default());
        let waiter = tokio::spawn({
            let queue = queue.clone();
            async move { queue.pop().await.name }
        });
        tokio::task::yield_now().await;
        queue.push(named("late".to_owned()));
        assert_eq!(waiter.await.unwrap(), "late");
    }
}
