//! S2: Event Queue.
//!
//! C++ lineage: `include/eafar/event_queue.hpp` + `src/event_queue.cpp`.
//!
//! The world changes ONLY through events. A handler reacts to an event by
//! writing fields and/or pushing follow-up events (cascade). New events are
//! appended to the tail and processed in strict FIFO order — deterministic
//! by construction (no timers, no unordered containers, no wall-clock).
//!
//! Handlers see only `{world fields, queue}` — never other handlers.
//! This is the coupling rule that S6 (dependency graph) formalizes.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::error::Result;
use crate::event::{Event, EventType, EVENT_TYPE_COUNT};
use crate::field_map::FieldMap;

/// Event handler callback (C++ `EventHandler` std::function).
///
/// C++ threw exceptions out of handlers; the Rust port returns
/// [`Result`] and `process_all` propagates the first error, aborting the
/// drain at exactly the same point.
pub type EventHandler = Arc<dyn Fn(&Event, &mut FieldMap, &mut EventQueue) -> Result<()>>;

/// FIFO event queue with per-type handler lists and an append-only history.
///
/// C++ lineage: `class EventQueue`.
#[derive(Default)]
pub struct EventQueue {
    queue: VecDeque<Event>,
    handlers: [Vec<EventHandler>; EVENT_TYPE_COUNT],
    history: Vec<Event>,
    next_seq: u64,
    processed: u64,
}

impl EventQueue {
    /// Append an event to the tail. It receives the next monotonic
    /// sequence number (C++ `push`; the invalid `_count` type is
    /// unrepresentable in Rust, so no runtime check is needed).
    pub fn push(&mut self, type_: EventType, x: u32, y: u32, payload: f32) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.queue.push_back(Event {
            type_,
            x,
            y,
            payload,
            seq,
        });
    }

    /// Register a handler for an event type. Handlers run in subscription
    /// order; multiple handlers per type all run (C++ `subscribe`).
    pub fn subscribe(
        &mut self,
        type_: EventType,
        handler: impl Fn(&Event, &mut FieldMap, &mut EventQueue) -> Result<()> + 'static,
    ) {
        self.handlers[type_ as usize].push(Arc::new(handler));
    }

    /// Remove every handler of every type (C++ `clear_handlers`).
    pub fn clear_handlers(&mut self) {
        for h in self.handlers.iter_mut() {
            h.clear();
        }
    }

    /// Process events until the queue is empty. Events pushed by handlers
    /// during processing are appended and processed in the same drain —
    /// strict FIFO, deterministic order (C++ `process_all`).
    pub fn process_all(&mut self, world: &mut FieldMap) -> Result<()> {
        while let Some(event) = self.queue.pop_front() {
            // History entry is recorded before handlers run, so cascaded
            // events appear after their cause — same as the C++ order.
            let type_idx = event.type_ as usize;
            self.history.push(event);
            self.processed += 1;

            // Re-read the handler list each iteration so a handler that
            // subscribes during processing is honored for later events,
            // mirroring the live iteration in C++ (without its UB on
            // reallocation).
            let mut i = 0usize;
            loop {
                let handler = {
                    let hs = &self.handlers[type_idx];
                    if i >= hs.len() {
                        break;
                    }
                    hs[i].clone()
                };
                handler(&event, world, self)?;
                i += 1;
            }
        }
        Ok(())
    }

    /// True when no events are pending.
    pub fn empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Number of currently pending events (C++ `pending_count`).
    pub fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Total number of processed events since creation
    /// (C++ `processed_count`).
    pub fn processed_count(&self) -> u64 {
        self.processed
    }

    /// Sequence number the next pushed event will receive
    /// (C++ `next_seq`).
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// Append-only log of every processed event (S9 replay / determinism)
    /// (C++ `history`).
    pub fn history(&self) -> &[Event] {
        &self.history
    }
}
