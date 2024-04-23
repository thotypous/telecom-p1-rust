use std::{f32::consts::PI, ops::Rem};

pub struct V21RX {
    // TODO: coloque outros atributos que você precisar aqui
    sampling_period: f32,
    samples_per_symbol: usize,
    omega_mark: f32,
    omega_space: f32,
}

impl V21RX {
    pub fn new(
        sampling_period: f32,
        samples_per_symbol: usize,
        omega_mark: f32,
        omega_space: f32,
    ) -> Self {
        // TODO: inicialize seus novos atributos abaixo
        Self {
            sampling_period,
            samples_per_symbol,
            omega_mark,
            omega_space,
        }
    }

    pub fn demodulate(&mut self, in_samples: &[f32], out_samples: &mut [u8]) {
        // TODO: seu código aqui
    }
}

pub struct V21TX {
    sampling_period: f32,
    omega_mark: f32,
    omega_space: f32,
    phase: f32,
}

impl V21TX {
    pub fn new(sampling_period: f32, omega_mark: f32, omega_space: f32) -> Self {
        Self {
            sampling_period,
            omega_mark,
            omega_space,
            phase: 0.,
        }
    }

    pub fn modulate(&mut self, in_samples: &[u8], out_samples: &mut [f32]) {
        debug_assert!(in_samples.len() == out_samples.len());

        for i in 0..in_samples.len() {
            out_samples[i] = self.phase.sin();

            let omega = if in_samples[i] == 0 {
                self.omega_space
            } else {
                self.omega_mark
            };
            self.phase = (self.phase + self.sampling_period * omega).rem(2. * PI);
        }
    }
}
