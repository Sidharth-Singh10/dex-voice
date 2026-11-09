//! clock and a presence/readiness probe supplied by the caller.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryState {
    Healthy,
    Recovering,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    None,
    EnterRecovering,
    Reopen,
    Fail,
}

pub struct DeviceRecovery {
    pub state: RecoveryState,
    pub deadline: Option<Instant>,
    pub attempts: u32,
    pub window: Duration,
    recovered_once: bool,
}

impl DeviceRecovery {
    pub fn new(window: Duration) -> Self {
        let window = if window < Duration::from_millis(500) {
            Duration::from_millis(500)
        } else {
            window
        };
        Self {
            state: RecoveryState::Healthy,
            deadline: None,
            attempts: 0,
            window,
            recovered_once: false,
        }
    }

    /// A loss edge was observed.
    pub fn on_loss(&mut self, now: Instant) -> RecoveryAction {
        match self.state {
            RecoveryState::Healthy => {
                if self.recovered_once {
                    // Second loss after a successful recovery: fail, no retry.
                    self.state = RecoveryState::Failed;
                    self.deadline = None;
                    RecoveryAction::Fail
                } else {
                    self.state = RecoveryState::Recovering;
                    self.deadline = Some(now + self.window);
                    self.attempts = 0;
                    RecoveryAction::EnterRecovering
                }
            }
            RecoveryState::Recovering => {
                // Second loss edge before healthy -> fail.
                self.state = RecoveryState::Failed;
                self.deadline = None;
                RecoveryAction::Fail
            }
            RecoveryState::Failed => RecoveryAction::None,
        }
    }

    /// Periodic tick during recovery. `present_ready` is true only when the
    /// deadline has been reached and the device resolved + is ready.
    pub fn tick(&mut self, now: Instant, present_ready: bool) -> RecoveryAction {
        if self.state != RecoveryState::Recovering {
            return RecoveryAction::None;
        }
        let deadline = match self.deadline {
            Some(d) => d,
            None => return RecoveryAction::None,
        };
        if now < deadline {
            // Defer: no reopen on early presence.
            return RecoveryAction::None;
        }
        if present_ready {
            self.attempts += 1;
            RecoveryAction::Reopen
        } else {
            self.state = RecoveryState::Failed;
            self.deadline = None;
            RecoveryAction::Fail
        }
    }

    pub fn reopen_result(&mut self, success: bool) {
        if self.state != RecoveryState::Recovering {
            return;
        }
        if success {
            self.state = RecoveryState::Healthy;
            self.deadline = None;
            self.recovered_once = true;
        } else {
            self.state = RecoveryState::Failed;
            self.deadline = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn loss_enters_recovering_and_clears() {
        let mut p = DeviceRecovery::new(Duration::from_secs(1));
        let now = t0();
        assert_eq!(p.on_loss(now), RecoveryAction::EnterRecovering);
        assert_eq!(p.state, RecoveryState::Recovering);
        assert_eq!(p.attempts, 0);
        assert!(p.deadline.is_some());
    }

    #[test]
    fn early_presence_does_not_reopen() {
        let mut p = DeviceRecovery::new(Duration::from_secs(1));
        let now = t0();
        p.on_loss(now);
        let early = now + Duration::from_millis(200);
        assert_eq!(p.tick(early, true), RecoveryAction::None);
        assert_eq!(p.attempts, 0);
    }

    #[test]
    fn deadline_ready_reopens_once() {
        let mut p = DeviceRecovery::new(Duration::from_secs(1));
        let now = t0();
        p.on_loss(now);
        let deadline = now + Duration::from_secs(1);
        assert_eq!(p.tick(deadline, true), RecoveryAction::Reopen);
        assert_eq!(p.attempts, 1);
        p.reopen_result(true);
        assert_eq!(p.state, RecoveryState::Healthy);
        // second tick no reopen
        assert_eq!(
            p.tick(deadline + Duration::from_millis(10), true),
            RecoveryAction::None
        );
    }

    #[test]
    fn reopen_failure_fails() {
        let mut p = DeviceRecovery::new(Duration::from_secs(1));
        let now = t0();
        p.on_loss(now);
        let deadline = now + Duration::from_secs(1);
        assert_eq!(p.tick(deadline, true), RecoveryAction::Reopen);
        p.reopen_result(false);
        assert_eq!(p.state, RecoveryState::Failed);
    }

    #[test]
    fn deadline_not_ready_fails_no_loop() {
        let mut p = DeviceRecovery::new(Duration::from_secs(1));
        let now = t0();
        p.on_loss(now);
        let deadline = now + Duration::from_secs(1);
        assert_eq!(p.tick(deadline, false), RecoveryAction::Fail);
        assert_eq!(p.state, RecoveryState::Failed);
        // no further attempts
        assert_eq!(
            p.tick(deadline + Duration::from_secs(1), true),
            RecoveryAction::None
        );
    }

    #[test]
    fn second_loss_after_healthy_fails_no_attempt() {
        let mut p = DeviceRecovery::new(Duration::from_secs(1));
        let now = t0();
        p.on_loss(now);
        let deadline = now + Duration::from_secs(1);
        assert_eq!(p.tick(deadline, true), RecoveryAction::Reopen);
        p.reopen_result(true);
        assert_eq!(p.state, RecoveryState::Healthy);
        let later = deadline + Duration::from_secs(2);
        assert_eq!(p.on_loss(later), RecoveryAction::Fail);
        assert_eq!(p.state, RecoveryState::Failed);
        assert_eq!(p.attempts, 1);
    }

    #[test]
    fn second_loss_before_healthy_fails() {
        let mut p = DeviceRecovery::new(Duration::from_secs(1));
        let now = t0();
        p.on_loss(now);
        let later = now + Duration::from_millis(200);
        assert_eq!(p.on_loss(later), RecoveryAction::Fail);
        assert_eq!(p.state, RecoveryState::Failed);
    }

    #[test]
    fn presence_flaps_no_attempt() {
        let mut p = DeviceRecovery::new(Duration::from_secs(1));
        let now = t0();
        p.on_loss(now);
        for i in 1..5 {
            let t = now + Duration::from_millis(i * 100);
            let _ = p.tick(t, i % 2 == 0);
        }
        // before deadline no reopen
        assert_eq!(p.attempts, 0);
    }

    #[test]
    fn min_window_enforced() {
        let p = DeviceRecovery::new(Duration::from_millis(100));
        assert!(p.window >= Duration::from_millis(500));
    }
}
