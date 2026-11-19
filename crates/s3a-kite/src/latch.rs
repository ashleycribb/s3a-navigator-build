//! The DD-144 consent latch (§5.2.1, DD-144): `epoch` +
//! `cleared_epoch` + `revokes_in_flight`, plus the cancellation-token
//! generation used to abort in-flight LRS POSTs on revoke.
//!
//! The inhibit is active iff `epoch != cleared_epoch`. `epoch` only
//! increases. A revoke increments `epoch` and the token generation at
//! receipt time (before the write mutex is taken) and again under the
//! mutex; an `on` records `(epoch, revokes_in_flight)` at receipt and
//! qualifies only when no revoke was in flight then, none is in flight
//! at the CAS, and `epoch` still equals the observed value. A
//! qualifying `on` clears the inhibit by a single CAS of
//! `cleared_epoch` to the observed epoch. A same-state `on` is a pure
//! no-op (never touches `cleared_epoch`). Every revoke decrements
//! `revokes_in_flight` at the end of its mutex hold (even a same-state
//! `off`, even on disconnect mid-hold); only a fail-closed exit skips
//! the decrement. A fail-closed exit also increments `epoch`.

use std::sync::atomic::{AtomicU64, Ordering};

/// The DD-144 latch + cancellation token (all interior mutability via
/// atomics; the latch is shared by the consent route, the LRS senders,
/// and the background retry worker).
pub struct Latch {
    /// Increments on every revoke receipt and again under the mutex.
    epoch: AtomicU64,
    /// The epoch value at which the inhibit was last cleared.
    cleared_epoch: AtomicU64,
    /// Revokes currently in flight (received but not yet finished
    /// under the mutex).
    revokes_in_flight: AtomicU64,
    /// Cancellation-token generation: increments on every revoke
    /// (receipt and mutex hold). An LRS sender registers the current
    /// generation and aborts when it changes.
    token_gen: AtomicU64,
}

impl Latch {
    pub fn new() -> Self {
        Latch {
            epoch: AtomicU64::new(0),
            cleared_epoch: AtomicU64::new(0),
            revokes_in_flight: AtomicU64::new(0),
            token_gen: AtomicU64::new(0),
        }
    }

    /// Is the inhibit active? `epoch != cleared_epoch`.
    pub fn inhibit_active(&self) -> bool {
        self.epoch.load(Ordering::SeqCst) != self.cleared_epoch.load(Ordering::SeqCst)
    }

    /// The current epoch (for the receipt observation of an `on`).
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// Revokes currently in flight (for the receipt observation of an
    /// `on` and the CAS qualification check).
    pub fn revokes_in_flight(&self) -> u64 {
        self.revokes_in_flight.load(Ordering::SeqCst)
    }

    /// Receipt-time revoke actions (before the write mutex is taken):
    /// increment `epoch`, the token generation, and
    /// `revokes_in_flight`. The token increment cancels in-flight LRS
    /// POSTs (senders poll the generation) and swaps in a fresh token.
    pub fn revoke_received(&self) {
        self.revokes_in_flight.fetch_add(1, Ordering::SeqCst);
        self.token_gen.fetch_add(1, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// Under-the-mutex revoke actions: re-cancel in-flight LRS POSTs
    /// and swap the token again (even for a same-state no-op `off`).
    pub fn revoke_under_mutex(&self) {
        self.token_gen.fetch_add(1, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// End of a revoke's mutex hold: decrement `revokes_in_flight`.
    /// Runs even for a same-state `off` and on disconnect mid-hold;
    /// only a fail-closed exit skips it.
    pub fn revoke_finished(&self) {
        self.revokes_in_flight.fetch_sub(1, Ordering::SeqCst);
    }

    /// A fail-closed exit also increments `epoch` (the process is
    /// exiting, so `revokes_in_flight` is not decremented).
    pub fn fail_closed_exit(&self) {
        self.token_gen.fetch_add(1, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// Record an `on` receipt: `(observed epoch, observed revokes)`.
    pub fn on_received(&self) -> (u64, u64) {
        (
            self.epoch.load(Ordering::SeqCst),
            self.revokes_in_flight.load(Ordering::SeqCst),
        )
    }

    /// Try to clear the inhibit for a qualifying `on`: succeeds only
    /// when `revokes_in_flight` is still 0 and `epoch` still equals
    /// the observed value. The clear is a single CAS of
    /// `cleared_epoch` to the observed epoch.
    pub fn try_clear_inhibit(&self, observed_epoch: u64) -> bool {
        if self.revokes_in_flight.load(Ordering::SeqCst) != 0 {
            return false;
        }
        if self.epoch.load(Ordering::SeqCst) != observed_epoch {
            return false;
        }
        let current = self.cleared_epoch.load(Ordering::SeqCst);
        if current == observed_epoch {
            return true;
        }
        self.cleared_epoch
            .compare_exchange(current, observed_epoch, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Register for an LRS send: the current token generation. A sender
    /// registers, re-checks inhibit inactive + flag on, and only then
    /// starts the POST; the POST aborts when the generation changes.
    pub fn register_token(&self) -> u64 {
        self.token_gen.load(Ordering::SeqCst)
    }

    /// Has the token changed since the given registration?
    pub fn token_changed(&self, registered: u64) -> bool {
        self.token_gen.load(Ordering::SeqCst) != registered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latch_basics() {
        let latch = Latch::new();
        assert!(!latch.inhibit_active());
        // A revoke activates the inhibit.
        latch.revoke_received();
        assert!(latch.inhibit_active());
        assert_eq!(latch.revokes_in_flight(), 1);
        let token = latch.register_token();
        latch.revoke_under_mutex();
        assert!(latch.token_changed(token));
        latch.revoke_finished();
        assert_eq!(latch.revokes_in_flight(), 0);
        assert!(latch.inhibit_active());
        // An on observed after the revoke qualifies and clears it.
        let (observed, revokes) = latch.on_received();
        assert_eq!(revokes, 0);
        assert!(latch.try_clear_inhibit(observed));
        assert!(!latch.inhibit_active());
    }

    #[test]
    fn latch_on_during_revoke_does_not_qualify() {
        let latch = Latch::new();
        latch.revoke_received();
        // An on received while the revoke is in flight observes a
        // revoke in flight and does not qualify.
        let (observed, revokes) = latch.on_received();
        assert_eq!(revokes, 1);
        assert!(!latch.try_clear_inhibit(observed));
        assert!(latch.inhibit_active());
        latch.revoke_under_mutex();
        latch.revoke_finished();
        // A later on (observed after the revoke finished) qualifies.
        let (observed, revokes) = latch.on_received();
        assert_eq!(revokes, 0);
        assert!(latch.try_clear_inhibit(observed));
        assert!(!latch.inhibit_active());
    }

    #[test]
    fn latch_same_state_on_is_noop() {
        let latch = Latch::new();
        // No revoke: the inhibit is already clear; a CAS to the same
        // epoch is a no-op success.
        let (observed, _) = latch.on_received();
        assert!(latch.try_clear_inhibit(observed));
        assert!(!latch.inhibit_active());
    }
}
