//! Boundary-only silence trimming for synthesized speech.
//!
//! Interior pauses are preserved; only leading and trailing quiet audio is
//! reduced, with a guard. The threshold is linear RMS.

#[derive(Debug)]
pub struct Trimmer {
    frame: usize,
    guard: usize,
    threshold: f32,
    pub input_samples: usize,
    pub output_samples: usize,
    pub started: bool,
    closed: bool,
    quiet: Vec<f32>,
    remainder: Vec<f32>,
}

impl Trimmer {
    pub fn new(sample_rate: u32, threshold: f32, guard_ms: u32) -> Self {
        assert!(sample_rate > 0);
        assert!(threshold > 0.0 && threshold < 1.0);
        Self {
            frame: (sample_rate / 100).max(1) as usize,
            guard: (sample_rate as u64 * guard_ms as u64 / 1000) as usize,
            threshold,
            input_samples: 0,
            output_samples: 0,
            started: false,
            closed: false,
            quiet: Vec::new(),
            remainder: Vec::new(),
        }
    }

    pub fn feed(&mut self, pcm: &[f32]) -> Vec<Vec<f32>> {
        assert!(!self.closed, "cannot append after finish");
        self.input_samples += pcm.len();
        let mut samples = Vec::with_capacity(self.remainder.len() + pcm.len());
        samples.extend_from_slice(&self.remainder);
        samples.extend_from_slice(pcm);
        let end = samples.len() / self.frame * self.frame;
        let mut output = Vec::new();
        for frame in samples[..end].chunks_exact(self.frame) {
            output.extend(self.consume(frame));
        }
        self.remainder = samples[end..].to_vec();
        output
    }

    pub fn finish(&mut self) -> Vec<Vec<f32>> {
        if self.closed {
            return Vec::new();
        }
        self.closed = true;
        let mut output = Vec::new();
        if !self.remainder.is_empty() {
            let remainder = std::mem::take(&mut self.remainder);
            output.extend(self.consume(&remainder));
        }
        if self.started && !self.quiet.is_empty() {
            let tail = self.quiet[..self.guard.min(self.quiet.len())].to_vec();
            if !tail.is_empty() {
                self.output_samples += tail.len();
                output.push(tail);
            }
        }
        self.quiet.clear();
        output
    }

    fn consume(&mut self, frame: &[f32]) -> Vec<Vec<f32>> {
        let rms = (frame.iter().map(|v| v * v).sum::<f32>() / frame.len() as f32).sqrt();
        if rms < self.threshold {
            self.quiet.extend_from_slice(frame);
            if !self.started && self.quiet.len() > self.guard {
                let excess = self.quiet.len() - self.guard;
                self.quiet.drain(..excess);
            }
            return Vec::new();
        }
        let mut output = Vec::new();
        if !self.quiet.is_empty() {
            output.push(std::mem::take(&mut self.quiet));
        }
        output.push(frame.to_vec());
        self.started = true;
        self.output_samples += output.iter().map(Vec::len).sum::<usize>();
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(trimmer: &mut Trimmer, chunks: &[Vec<f32>]) -> Vec<f32> {
        let mut output = Vec::new();
        for chunk in chunks {
            for piece in trimmer.feed(chunk) {
                output.extend(piece);
            }
        }
        for piece in trimmer.finish() {
            output.extend(piece);
        }
        output
    }

    #[test]
    fn trims_boundaries_and_keeps_interior_pauses() {
        let samples = [
            vec![0.0; 50],
            vec![1.0; 20],
            vec![0.0; 60],
            vec![1.0; 20],
            vec![0.0; 200],
        ]
        .concat();
        let mut trimmer = Trimmer::new(1000, 0.01, 20);
        let output = collect(&mut trimmer, &[samples]);
        assert_eq!(output.len(), 20 + 20 + 60 + 20 + 20);
        assert_eq!(trimmer.output_samples, output.len());
    }

    #[test]
    fn all_quiet_is_not_mistaken_for_speech() {
        let mut trimmer = Trimmer::new(1000, 0.01, 20);
        let output = collect(&mut trimmer, &[vec![0.0; 100]]);
        assert!(output.is_empty());
        assert!(!trimmer.started);
    }

    #[test]
    fn chunking_does_not_change_the_result() {
        let samples = [vec![0.0; 31], vec![1.0; 27], vec![0.0; 48]].concat();
        let mut once = Trimmer::new(1000, 0.01, 10);
        let a = collect(&mut once, std::slice::from_ref(&samples));
        let mut split = Trimmer::new(1000, 0.01, 10);
        let chunks: Vec<Vec<f32>> = samples.chunks(7).map(|c| c.to_vec()).collect();
        let b = collect(&mut split, &chunks);
        assert_eq!(a, b);
    }
}
