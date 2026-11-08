//! Single-active-turn bookkeeping.
//!
//! At most one phrase may be in the channel or synthesizing at a time. The
//! owner token travels with the phrase and releases the slot only when it is
//! consumed or dropped.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Result of offering a phrase to the speech worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    Busy,
    Disconnected,
}

/// One phrase travelling to the speech worker, together with the token that
/// owns the single in-flight slot. The slot is released when the worker drops
/// the token after consuming (or cancelling) the phrase.
#[derive(Debug)]
pub struct Dispatch {
    pub id: u64,
    pub words: Vec<String>,
    pub at: Instant,
    pub token: TurnToken,
}

/// Shared state for one dispatched phrase.
#[derive(Debug)]
pub struct TurnControl {
    pub id: u64,
    cancelled: AtomicBool,
    finished: AtomicBool,
    first_pcm: AtomicBool,
    pub created: Instant,
}

impl TurnControl {
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    pub fn has_pcm(&self) -> bool {
        self.first_pcm.load(Ordering::SeqCst)
    }

    pub fn mark_pcm(&self) {
        self.first_pcm.store(true, Ordering::SeqCst);
    }

    fn cancel(&self) -> bool {
        !self.cancelled.swap(true, Ordering::SeqCst)
    }
}

/// The single in-flight slot.
#[derive(Debug, Default)]
pub struct InFlight {
    next_id: AtomicU64,
    active: Mutex<Option<Arc<TurnControl>>>,
    channel_empty: AtomicBool,
}

impl InFlight {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            next_id: AtomicU64::new(1),
            active: Mutex::new(None),
            channel_empty: AtomicBool::new(true),
        })
    }

    /// Reserves the slot. Returns `None` when a turn is already active.
    pub fn begin(&self, _now: Instant) -> Option<Arc<TurnControl>> {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if active.is_some() {
            return None;
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let control = Arc::new(TurnControl {
            id,
            cancelled: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            first_pcm: AtomicBool::new(false),
            created: _now,
        });
        *active = Some(Arc::clone(&control));
        self.channel_empty.store(false, Ordering::SeqCst);
        Some(control)
    }

    /// Called by the worker when it receives the phrase.
    pub fn received(&self) {
        self.channel_empty.store(true, Ordering::SeqCst);
    }

    pub fn channel_empty(&self) -> bool {
        self.channel_empty.load(Ordering::SeqCst)
    }

    pub fn active(&self) -> Option<Arc<TurnControl>> {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn is_active(&self) -> bool {
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Requests cancellation of the active turn only.
    pub fn cancel_active(&self) -> bool {
        let active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        active.as_ref().is_some_and(|turn| turn.cancel())
    }

    /// Releases the slot if `id` still matches. Returns true on release.
    pub fn release(&self, id: u64) -> bool {
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if active.as_ref().is_some_and(|turn| turn.id == id) {
            *active = None;
            self.channel_empty.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Drops a turn that was never sent (dispatch failure).
    pub fn release_failed(&self, id: u64) {
        self.release(id);
    }
}

/// RAII owner of an in-flight slot.
#[derive(Debug)]
pub struct TurnToken {
    inflight: Arc<InFlight>,
    control: Arc<TurnControl>,
}

impl TurnToken {
    pub fn new(inflight: Arc<InFlight>, control: Arc<TurnControl>) -> Self {
        Self { inflight, control }
    }

    pub fn control(&self) -> &Arc<TurnControl> {
        &self.control
    }

    pub fn id(&self) -> u64 {
        self.control.id
    }

    pub fn finished(&self) -> bool {
        self.control.finished()
    }

    pub fn has_pcm(&self) -> bool {
        self.control.has_pcm()
    }

    pub fn received(&self) {
        self.inflight.received();
    }

    pub fn mark_pcm(&self) {
        self.control.mark_pcm();
    }

    pub fn cancel_requested(&self) -> bool {
        self.control.cancelled()
    }
}

impl Drop for TurnToken {
    fn drop(&mut self) {
        self.control.finished.store(true, Ordering::SeqCst);
        self.inflight.release(self.control.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_turn_can_be_active() {
        let inflight = InFlight::new();
        let now = Instant::now();
        let first = inflight.begin(now).expect("first turn");
        assert!(inflight.begin(now).is_none());
        let token = TurnToken::new(Arc::clone(&inflight), first);
        assert!(!inflight.channel_empty());
        token.received();
        assert!(inflight.channel_empty());
        drop(token);
        assert!(!inflight.is_active());
        assert!(inflight.begin(now).is_some());
    }

    #[test]
    fn stale_cancel_cannot_touch_a_new_turn() {
        let inflight = InFlight::new();
        let first = inflight.begin(Instant::now()).unwrap();
        assert!(inflight.cancel_active());
        drop(TurnToken::new(Arc::clone(&inflight), Arc::clone(&first)));
        let second = inflight.begin(Instant::now()).unwrap();
        assert_ne!(first.id, second.id);
        assert!(!second.cancelled());
    }

    #[test]
    fn release_is_idempotent_and_id_scoped() {
        let inflight = InFlight::new();
        let first = inflight.begin(Instant::now()).unwrap();
        assert!(inflight.release(first.id));
        assert!(!inflight.release(first.id));
    }
}
