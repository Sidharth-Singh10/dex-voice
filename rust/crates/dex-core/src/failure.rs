//! Failure domains and supervisory state machines.
//!
//! Synthesis problems and audio-device problems are kept separate all the way
//! to the UI: a stalled synthesiser must never trigger a device reopen, and a
//! lost device must not be reported as a speech failure.

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::turn::TurnControl;

/// A turn that produces no PCM within this window is cancelled and watched.
pub const NO_PCM_CANCEL_MS: u64 = 2_000;

/// Which subsystem owns a terminal failure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureDomain {
    #[default]
    None,
    Synthesis,
    Device,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SynthesisState {
    #[default]
    Idle,
    Synthesizing,
    /// Cancellation was requested but the worker has not acknowledged it.
    Stalled,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    #[default]
    Ready,
    Recovering,
    Lost,
}

/// What a disconnected speech worker should do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisconnectAction {
    /// Restart the worker once. Only allowed with no live turn and an empty
    /// dispatch channel.
    Restart,
    Fail,
}

/// What a playback stall should do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceAction {
    /// Drop and reopen playback once, then clear the queue.
    Reopen,
    Fail,
}

/// Synthesis/device supervisory state shared by the engine and the app.
#[derive(Clone, Debug)]
pub struct Supervisor {
    pub failure: FailureDomain,
    pub synthesis: SynthesisState,
    pub device: DeviceState,
    tts_restarts: u32,
    device_recoveries: u32,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self {
            failure: FailureDomain::None,
            synthesis: SynthesisState::Idle,
            device: DeviceState::Ready,
            tts_restarts: 0,
            device_recoveries: 0,
        }
    }
}

impl Supervisor {
    pub fn new() -> Self {
        Self::default()
    }

    /// A dispatch channel disconnected. Restart the worker exactly once, and
    /// only when no phrase is live and nothing is still queued.
    pub fn on_disconnect(&mut self, live_token: bool, channel_drained: bool) -> DisconnectAction {
        if !live_token && channel_drained && self.tts_restarts == 0 {
            self.tts_restarts = 1;
            self.synthesis = SynthesisState::Idle;
            DisconnectAction::Restart
        } else {
            self.fail_synthesis(SynthesisState::Failed);
            DisconnectAction::Fail
        }
    }

    /// Cancellation was requested but the worker did not acknowledge within
    /// the timeout. This is a synthesis stall, never a device problem.
    pub fn on_cancel_timeout(&mut self) -> bool {
        self.fail_synthesis(SynthesisState::Stalled);
        true
    }

    pub fn on_cancel_requested(&mut self) {
        if self.synthesis != SynthesisState::Failed {
            self.synthesis = SynthesisState::Idle;
        }
    }

    pub fn on_playback_stall(&mut self) -> DeviceAction {
        if self.device_recoveries == 0 {
            self.device_recoveries = 1;
            self.device = DeviceState::Recovering;
            DeviceAction::Reopen
        } else {
            self.device = DeviceState::Lost;
            self.failure = FailureDomain::Device;
            DeviceAction::Fail
        }
    }

    pub fn on_playback_recovered(&mut self) {
        if self.device == DeviceState::Recovering {
            self.device = DeviceState::Ready;
        }
    }

    pub fn fail_synthesis(&mut self, state: SynthesisState) {
        self.failure = FailureDomain::Synthesis;
        self.synthesis = state;
    }

    pub fn fail_device(&mut self) {
        self.failure = FailureDomain::Device;
        self.device = DeviceState::Lost;
    }

    pub fn tts_restarts(&self) -> u32 {
        self.tts_restarts
    }

    pub fn device_recoveries(&self) -> u32 {
        self.device_recoveries
    }
}

/// Deadline for a cancellation acknowledgement.
#[derive(Clone, Copy, Debug)]
pub struct CancelWatch {
    deadline: Instant,
}

impl CancelWatch {
    pub fn new(now: Instant, timeout: Duration) -> Self {
        Self {
            deadline: now + timeout,
        }
    }

    pub fn expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }
}

/// Stop terminal states. A bounded stop that could not finish cleanly is
/// visible rather than silent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopOutcome {
    Idle,
    Recovering,
    Incomplete,
}

pub fn stop_outcome(device_recovering: bool, timed_out: bool) -> StopOutcome {
    if timed_out {
        StopOutcome::Incomplete
    } else if device_recovering {
        StopOutcome::Recovering
    } else {
        StopOutcome::Idle
    }
}

/// What the production tick should do about the current turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnWatchAction {
    None,
    /// The turn produced no PCM within the deadline: cancel this exact turn
    /// and start the acknowledgement watch.
    Cancel {
        id: u64,
    },
    /// The cancel was not acknowledged. The synthesis domain is stalled.
    Stalled {
        id: u64,
    },
}

/// Production no-PCM cancel watchdog.
///
/// Drives the 2 s no-PCM deadline from [`TurnControl::created`] and the 1.5 s
/// acknowledgement watch through [`CancelWatch`]. It owns the [`Supervisor`]
/// so the failure domain and synthesis state are the same values the UI sees.
#[derive(Clone, Debug)]
pub struct TurnWatch {
    supervisor: Supervisor,
    no_pcm: Duration,
    ack: Duration,
    cancel_id: Option<u64>,
    watch: Option<CancelWatch>,
}

impl TurnWatch {
    pub fn new(no_pcm: Duration, ack: Duration) -> Self {
        Self {
            supervisor: Supervisor::new(),
            no_pcm,
            ack,
            cancel_id: None,
            watch: None,
        }
    }

    pub fn supervisor(&self) -> &Supervisor {
        &self.supervisor
    }

    pub fn supervisor_mut(&mut self) -> &mut Supervisor {
        &mut self.supervisor
    }

    /// Observes one production tick. `active` is the current turn, if any.
    pub fn tick(&mut self, now: Instant, active: Option<&TurnControl>) -> TurnWatchAction {
        let Some(turn) = active else {
            self.cancel_id = None;
            self.watch = None;
            return TurnWatchAction::None;
        };

        if self.cancel_id == Some(turn.id) {
            if self.watch.as_ref().is_some_and(|watch| watch.expired(now)) {
                self.supervisor.on_cancel_timeout();
                return TurnWatchAction::Stalled { id: turn.id };
            }
            return TurnWatchAction::None;
        }

        // A different turn: drop any stale watch before evaluating it.
        self.cancel_id = None;
        self.watch = None;
        if !turn.has_pcm() && now.saturating_duration_since(turn.created) >= self.no_pcm {
            self.supervisor.on_cancel_requested();
            self.cancel_id = Some(turn.id);
            self.watch = Some(CancelWatch::new(now, self.ack));
            return TurnWatchAction::Cancel { id: turn.id };
        }
        TurnWatchAction::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tts_restart_is_allowed_once_and_never_with_a_live_turn() {
        let mut supervisor = Supervisor::new();
        assert_eq!(supervisor.on_disconnect(true, true), DisconnectAction::Fail);
        assert_eq!(supervisor.failure, FailureDomain::Synthesis);

        let mut supervisor = Supervisor::new();
        assert_eq!(
            supervisor.on_disconnect(false, false),
            DisconnectAction::Fail
        );

        let mut supervisor = Supervisor::new();
        assert_eq!(
            supervisor.on_disconnect(false, true),
            DisconnectAction::Restart
        );
        assert_eq!(
            supervisor.on_disconnect(false, true),
            DisconnectAction::Fail
        );
    }

    #[test]
    fn cancel_timeout_is_a_synthesis_stall_not_a_device_failure() {
        let mut supervisor = Supervisor::new();
        supervisor.on_cancel_timeout();
        assert_eq!(supervisor.failure, FailureDomain::Synthesis);
        assert_eq!(supervisor.synthesis, SynthesisState::Stalled);
        assert_eq!(supervisor.device, DeviceState::Ready);
    }

    #[test]
    fn playback_stall_reopens_once_then_fails_as_device() {
        let mut supervisor = Supervisor::new();
        assert_eq!(supervisor.on_playback_stall(), DeviceAction::Reopen);
        assert_eq!(supervisor.device, DeviceState::Recovering);
        supervisor.on_playback_recovered();
        assert_eq!(supervisor.device, DeviceState::Ready);
        // P0 allows exactly one reopen per engine generation.
        assert_eq!(supervisor.on_playback_stall(), DeviceAction::Fail);
        assert_eq!(supervisor.failure, FailureDomain::Device);
        assert_eq!(supervisor.device, DeviceState::Lost);
    }

    #[test]
    fn stop_terminal_state_is_visible() {
        assert_eq!(stop_outcome(false, false), StopOutcome::Idle);
        assert_eq!(stop_outcome(true, false), StopOutcome::Recovering);
        assert_eq!(stop_outcome(false, true), StopOutcome::Incomplete);
    }

    #[test]
    fn cancel_watch_expires_only_at_the_deadline() {
        let now = Instant::now();
        let watch = CancelWatch::new(now, Duration::from_millis(1500));
        assert!(!watch.expired(now + Duration::from_millis(1499)));
        assert!(watch.expired(now + Duration::from_millis(1500)));
    }
}
