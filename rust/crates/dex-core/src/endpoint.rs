//! Audio-frame phrase endpointing.
//!
//! Endpoint decisions are made on fixed 20 ms frames of audio, never on wall
//! clock reads between blocks. A phrase may be armed by voiced audio or by any
//! non-empty per-phrase display text, which is what lets a single word with no
//! committed trailing space still reach the silence endpoint.

use std::time::Duration;

/// Endpoint tuning. Defaults are the approved P0 values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EndpointConfig {
    pub frame_ms: u64,
    /// Linear RMS at or above which a frame counts as speech.
    pub speech_rms: f32,
    /// Linear RMS below which a frame counts as silence (hysteresis gap).
    pub quiet_rms: f32,
    pub silence_ms: u64,
    pub cap_ms: u64,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            frame_ms: 20,
            speech_rms: 0.008,
            quiet_rms: 0.003,
            silence_ms: 800,
            cap_ms: 10_000,
        }
    }
}

impl EndpointConfig {
    pub fn frame_duration(&self) -> Duration {
        Duration::from_millis(self.frame_ms)
    }

    pub fn frames_for(&self, ms: u64) -> u64 {
        ms.div_ceil(self.frame_ms.max(1))
    }
}

/// Why a phrase ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointReason {
    /// Enough consecutive audio silence after the phrase was armed.
    Silence,
    /// The uninterrupted-speech cap, anchored at the first armed frame.
    Cap,
}

/// What the loop should do after one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointEvent {
    Continue,
    /// A phrase was armed and reached a boundary: finalize and emit.
    Endpoint(EndpointReason),
    /// The phrase never armed and the cap elapsed: reset the stream without
    /// finalizing or emitting anything.
    UnarmedReset,
}

/// Per-phrase endpoint state driven exclusively by audio frames.
#[derive(Clone, Debug)]
pub struct PhraseClock {
    config: EndpointConfig,
    frames: u64,
    armed_at: Option<u64>,
    speech_seen: bool,
    quiet_frames: u64,
}

impl PhraseClock {
    pub fn new(config: EndpointConfig) -> Self {
        Self {
            config,
            frames: 0,
            armed_at: None,
            speech_seen: false,
            quiet_frames: 0,
        }
    }

    pub fn config(&self) -> EndpointConfig {
        self.config
    }

    /// Drops all per-phrase state. The next phrase starts a fresh cap window.
    pub fn reset_phrase(&mut self) {
        self.frames = 0;
        self.armed_at = None;
        self.speech_seen = false;
        self.quiet_frames = 0;
    }

    pub fn is_armed(&self) -> bool {
        self.armed_at.is_some()
    }

    pub fn armed_at_frame(&self) -> Option<u64> {
        self.armed_at
    }

    /// Feeds one audio frame. `display_nonempty` is the per-phrase display
    /// snapshot for this frame.
    pub fn push_frame(&mut self, rms: f32, display_nonempty: bool) -> EndpointEvent {
        self.frames += 1;
        if rms >= self.config.speech_rms {
            self.speech_seen = true;
        }
        if self.armed_at.is_none() && (self.speech_seen || display_nonempty) {
            self.armed_at = Some(self.frames);
        }

        let Some(armed) = self.armed_at else {
            if self.frames * self.config.frame_ms >= self.config.cap_ms {
                return EndpointEvent::UnarmedReset;
            }
            return EndpointEvent::Continue;
        };

        if rms < self.config.quiet_rms {
            self.quiet_frames += 1;
        } else {
            // Values between quiet and speech are ambiguous: they neither arm a
            // fresh phrase nor continue a silence run.
            self.quiet_frames = 0;
        }

        if self.quiet_frames * self.config.frame_ms >= self.config.silence_ms {
            return EndpointEvent::Endpoint(EndpointReason::Silence);
        }
        if (self.frames - armed) * self.config.frame_ms >= self.config.cap_ms {
            return EndpointEvent::Endpoint(EndpointReason::Cap);
        }
        EndpointEvent::Continue
    }
}

/// Re-blocks arbitrary capture buffers into fixed-width frames.
#[derive(Debug)]
pub struct FrameSplitter {
    width: usize,
    buffer: Vec<f32>,
}

impl FrameSplitter {
    pub fn new(width: usize) -> Self {
        assert!(width > 0);
        Self {
            width,
            buffer: Vec::with_capacity(width),
        }
    }

    pub fn push(&mut self, pcm: &[f32], mut on_frame: impl FnMut(&[f32])) {
        self.buffer.extend_from_slice(pcm);
        while self.buffer.len() >= self.width {
            let rest = self.buffer.split_off(self.width);
            let frame = std::mem::replace(&mut self.buffer, rest);
            on_frame(&frame);
        }
    }

    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }
}

/// Linear RMS of a frame.
pub fn rms(frame: &[f32]) -> f32 {
    (frame.iter().map(|v| v * v).sum::<f32>() / frame.len().max(1) as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(clock: &mut PhraseClock, frames: usize, rms: f32, display: bool) -> Vec<EndpointEvent> {
        (0..frames)
            .map(|_| clock.push_frame(rms, display))
            .filter(|event| *event != EndpointEvent::Continue)
            .collect()
    }

    #[test]
    fn display_text_arms_the_one_word_phrase() {
        let mut clock = PhraseClock::new(EndpointConfig::default());
        // One frame of committed text with no trailing space shows up only in
        // display(); 800 ms of silence must still end the phrase.
        let events = run(&mut clock, 40, 0.001, true);
        assert_eq!(
            events,
            vec![EndpointEvent::Endpoint(EndpointReason::Silence)]
        );
        assert_eq!(clock.armed_at_frame(), Some(1));
    }

    #[test]
    fn cap_is_anchored_at_the_first_armed_frame() {
        let mut clock = PhraseClock::new(EndpointConfig::default());
        assert_eq!(clock.push_frame(0.001, false), EndpointEvent::Continue);
        assert_eq!(clock.push_frame(0.5, false), EndpointEvent::Continue);
        assert_eq!(clock.armed_at_frame(), Some(2));
        let mut events = Vec::new();
        for _ in 0..600 {
            let event = clock.push_frame(0.5, false);
            if event != EndpointEvent::Continue {
                events.push(event);
                break;
            }
        }
        assert_eq!(events, vec![EndpointEvent::Endpoint(EndpointReason::Cap)]);
    }

    #[test]
    fn unarmed_phrase_resets_without_an_endpoint() {
        let mut clock = PhraseClock::new(EndpointConfig::default());
        let events = run(&mut clock, 500, 0.001, false);
        assert_eq!(events, vec![EndpointEvent::UnarmedReset]);
        assert!(!clock.is_armed());
    }

    #[test]
    fn hysteresis_gap_neither_arms_nor_counts_as_silence() {
        let mut clock = PhraseClock::new(EndpointConfig::default());
        for _ in 0..100 {
            assert_eq!(clock.push_frame(0.005, false), EndpointEvent::Continue);
        }
        assert!(!clock.is_armed());
    }

    #[test]
    fn splitter_emits_only_full_frames() {
        let mut splitter = FrameSplitter::new(4);
        let mut frames = Vec::new();
        splitter.push(&[1.0, 2.0, 3.0], |frame| frames.push(frame.to_vec()));
        assert!(frames.is_empty());
        assert_eq!(splitter.buffered(), 3);
        splitter.push(&[4.0, 5.0, 6.0], |frame| frames.push(frame.to_vec()));
        assert_eq!(frames, vec![vec![1.0, 2.0, 3.0, 4.0]]);
        assert_eq!(splitter.buffered(), 2);
    }
}
