use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use crate::audio::source::AudioSource;

const BAND_COUNT: usize = 16;
const BAND_RANGES: [(f32, f32); BAND_COUNT] = [
    (35.0, 55.0),
    (55.0, 80.0),
    (80.0, 115.0),
    (115.0, 165.0),
    (165.0, 240.0),
    (240.0, 350.0),
    (350.0, 510.0),
    (510.0, 740.0),
    (740.0, 1_050.0),
    (1_050.0, 1_450.0),
    (1_450.0, 1_950.0),
    (1_950.0, 2_550.0),
    (2_550.0, 3_250.0),
    (3_250.0, 4_050.0),
    (4_050.0, 4_850.0),
    (4_850.0, 5_800.0),
];

#[derive(Clone)]
pub struct PcmMeter {
    bands: Arc<[AtomicU32; BAND_COUNT]>,
}

impl Default for PcmMeter {
    fn default() -> Self {
        Self {
            bands: Arc::new(std::array::from_fn(|_| AtomicU32::new(0))),
        }
    }
}

impl PcmMeter {
    pub fn levels(&self) -> [f32; BAND_COUNT] {
        std::array::from_fn(|index| self.bands[index].load(Ordering::Relaxed) as f32 / 1_000.0)
    }

    pub fn clear(&self) {
        for band in self.bands.iter() {
            band.store(0, Ordering::Relaxed);
        }
    }

    pub fn analyze_f64(&self, samples: &[f64], channels: usize, sample_rate: u32) {
        self.analyze(samples.len(), channels, sample_rate, |index| {
            samples[index] as f32
        });
    }

    pub fn analyze_f32(&self, samples: &[f32], channels: usize, sample_rate: u32) {
        self.analyze(samples.len(), channels, sample_rate, |index| samples[index]);
    }

    fn analyze(
        &self,
        sample_count: usize,
        channels: usize,
        sample_rate: u32,
        mut sample_at: impl FnMut(usize) -> f32,
    ) {
        if channels == 0 || sample_rate == 0 {
            self.clear();
            return;
        }

        let frames = (sample_count / channels).min(2_048);
        if frames < 8 {
            return;
        }

        let frame_stride = (sample_count / channels).saturating_sub(frames);
        let nyquist = sample_rate as f32 / 2.0;
        for (band, (low, high)) in BAND_RANGES.iter().enumerate() {
            let high = (*high).min(nyquist - 1.0).max(1.0);
            let low = (*low).min(high).max(1.0);
            let middle = (low * high).sqrt();
            let mut level = 0.0_f32;

            for frequency in [low, middle, high] {
                let power = goertzel_power(
                    frames,
                    frame_stride,
                    channels,
                    sample_rate,
                    frequency,
                    &mut sample_at,
                );
                level += (power.sqrt() / frames as f32 * 18.0).clamp(0.0, 1.0);
            }
            level = (level / 3.0).clamp(0.0, 1.0);
            let previous = self.bands[band].load(Ordering::Relaxed) as f32 / 1_000.0;
            let smoothed = if level > previous {
                level
            } else {
                previous * 0.72 + level * 0.28
            };
            self.bands[band].store((smoothed * 1_000.0).round() as u32, Ordering::Relaxed);
        }
    }
}

fn goertzel_power(
    frames: usize,
    frame_stride: usize,
    channels: usize,
    sample_rate: u32,
    frequency: f32,
    sample_at: &mut impl FnMut(usize) -> f32,
) -> f32 {
    let omega = 2.0 * std::f32::consts::PI * frequency / sample_rate as f32;
    let coefficient = 2.0 * omega.cos();
    let mut prev = 0.0;
    let mut prev2 = 0.0;

    for frame in 0..frames {
        let base = (frame_stride + frame) * channels;
        let mut mono = 0.0;
        for channel in 0..channels {
            mono += sample_at(base + channel);
        }
        mono /= channels as f32;

        let next = mono + coefficient * prev - prev2;
        prev2 = prev;
        prev = next;
    }

    (prev2 * prev2 + prev * prev - coefficient * prev * prev2).max(0.0)
}

pub struct MeteredSource<S> {
    source: S,
    meter: PcmMeter,
}

impl<S> MeteredSource<S>
where
    S: AudioSource,
{
    pub fn new(source: S, meter: PcmMeter) -> Self {
        Self { source, meter }
    }
}

impl<S> AudioSource for MeteredSource<S>
where
    S: AudioSource,
{
    fn write(&mut self, output: &mut [f32]) -> usize {
        let written = self.source.write(output);
        self.meter
            .analyze_f32(&output[..written], self.channel_count(), self.sample_rate());
        written
    }

    fn channel_count(&self) -> usize {
        self.source.channel_count()
    }

    fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }
}
