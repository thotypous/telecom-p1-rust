use std::{f32::consts::PI, ops::Rem};

pub struct V21TX {
    sampling_period: f32,
    channels: usize,
    omega_mark: f32,
    omega_space: f32,
    phase: f32,
}

impl V21TX {
    pub fn new(sampling_period: f32, channels: usize, omega_mark: f32, omega_space: f32) -> Self {
        Self {
            sampling_period,
            channels,
            omega_mark,
            omega_space,
            phase: 0.,
        }
    }

    pub fn modulate(&mut self, in_samples: &[u8], out_samples: &mut [f32]) {
        let mut j = 0;
        for i in 0..in_samples.len() {
            let sample = self.phase.sin();
            for _ in 0..self.channels {
                out_samples[j] = sample;
                j += 1;
            }
            let omega = if in_samples[i] == 0 {
                self.omega_space
            } else {
                self.omega_mark
            };
            self.phase = (self.phase + self.sampling_period * omega).rem(2. * PI);
        }
    }
}
