use std::collections::VecDeque;

pub struct UartTx {
    samples_per_symbol: u32,
    samples: VecDeque<u8>,
}

impl UartTx {
    pub fn new(samples_per_symbol: u32) -> Self {
        Self {
            samples_per_symbol,
            samples: VecDeque::new(),
        }
    }

    fn put_bit(&mut self, bit: u8) {
        for _ in 0..self.samples_per_symbol {
            self.samples.push_back(bit);
        }
    }

    pub fn put_byte(&mut self, mut byte: u8) {
        self.put_bit(0); // start bit
        for _ in 0..8 {
            self.put_bit(byte & 1);
            byte >>= 1;
        }
        self.put_bit(1); // stop bit
    }

    pub fn get_samples(&mut self, buffer: &mut [u8]) {
        for i in 0..buffer.len() {
            buffer[i] = self.samples.pop_front().unwrap_or(1);
        }
    }
}
