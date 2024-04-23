use crossbeam_channel::unbounded;
use interp1d::Interp1d;
use modem::{
    uart::{UartRx, UartTx},
    v21::{V21RX, V21TX},
};
use rand::SeedableRng;
use rand::{
    distributions::{uniform::Uniform, Distribution},
    Rng,
};
use rand_pcg;

const BAUD_RATE: u32 = 300;

#[test]
fn uart_trivial_48000() {
    test_uart(48000, false, false);
}

#[test]
fn uart_trivial_44100() {
    test_uart(44100, false, false);
}

#[test]
fn uart_unsync_48000() {
    test_uart(48000, false, true);
}

#[test]
fn uart_unsync_44100() {
    test_uart(44100, false, true);
}

#[test]
fn uart_noisy_48000() {
    test_uart(48000, true, false);
}

#[test]
fn uart_noisy_44100() {
    test_uart(44100, true, false);
}

#[test]
fn uart_noisy_unsync_48000() {
    test_uart(48000, true, true);
}

#[test]
fn uart_noisy_unsync_44100() {
    test_uart(44100, true, true);
}

fn test_uart(srate: u32, add_noise: bool, add_timing_offset: bool) {
    let samples_per_symbol = (srate / BAUD_RATE) as usize;

    let (rx_sender, rx_receiver) = unbounded();

    let mut uart_tx = UartTx::new(samples_per_symbol as u32);
    let mut uart_rx = UartRx::new(samples_per_symbol as u32, rx_sender);

    let mut gen = rand_pcg::Pcg32::seed_from_u64(42);
    let d_idle_samples = Uniform::new(0, samples_per_symbol);
    let d_msg_bytes = Uniform::new(1, 100);
    let d_byte = Uniform::new(0, 255);
    let d_timing_offset = Uniform::new(0.98, 1.02);

    for iteration in 0..100 {
        let idle_samples = d_idle_samples.sample(&mut gen);
        let msg_bytes = d_msg_bytes.sample(&mut gen);
        let msg_samples = 10 * samples_per_symbol * msg_bytes;
        let n = idle_samples + msg_samples;

        let mut transmitted_samples = vec![0; n];
        uart_tx.get_samples(&mut transmitted_samples[..idle_samples]);

        let orig_msg: Vec<u8> = d_byte.sample_iter(&mut gen).take(msg_bytes).collect();
        for b in &orig_msg {
            uart_tx.put_byte(*b);
        }
        uart_tx.get_samples(&mut transmitted_samples[idle_samples..]);

        let timing_offset = if add_timing_offset {
            d_timing_offset.sample(&mut gen)
        } else {
            1.0
        };

        let received_samples = bs_transition_channel(
            &mut gen,
            if add_noise { 0.5 } else { 0.0 },
            if add_noise { samples_per_symbol / 4 } else { 0 },
            timing_offset,
            &transmitted_samples,
        );

        let d_cut = Uniform::new(1, received_samples.len() - 1);
        let cut = d_cut.sample(&mut gen);
        uart_rx.put_samples(&received_samples[..cut]);
        uart_rx.put_samples(&received_samples[cut..]);

        assert_eq!(
            rx_receiver.try_iter().collect::<Vec<u8>>(),
            orig_msg,
            "wrong contents on iteration {}",
            iteration,
        );
    }
}

fn bs_transition_channel<R: Rng + ?Sized>(
    gen: &mut R,
    flip_probability: f32,
    samples_affected_on_transition: usize,
    timing_offset: f32,
    samples: &[u8],
) -> Vec<u8> {
    let nxd = samples.len();
    let mut yd = vec![0.0; nxd];

    let d = Uniform::new(0.0, 1.0);
    let mut previous_sample = samples[0];

    let mut i = 0;
    while i < nxd {
        yd[i] = samples[i] as f32;
        if samples[i] != previous_sample && samples_affected_on_transition > 0 {
            // transition, apply BSC model
            let e = (i + samples_affected_on_transition).min(nxd);
            for j in i..e {
                if d.sample(gen) < flip_probability {
                    yd[j] = if samples[j] == 0 { 1.0 } else { 0.0 };
                } else {
                    yd[j] = if samples[j] == 0 { 0.0 } else { 1.0 };
                }
            }
            i = e - 1;
        }
        previous_sample = samples[i];
        i += 1;
    }

    let yi = apply_timing_offset(timing_offset, &yd);
    yi.iter()
        .map(|value| if *value > 0.5 { 1 } else { 0 })
        .collect::<Vec<u8>>()
}

fn apply_timing_offset(timing_offset: f32, yd: &[f32]) -> Vec<f32> {
    let nxd = yd.len();
    let ni = ((nxd as f32 - 1.0) / timing_offset) as usize + 1;
    let xd = (0..nxd)
        .map(|i| i as f32 / (nxd as f32 - 1.0))
        .collect::<Vec<f32>>();
    let interpolator = Interp1d::new_sorted(xd, yd.to_vec()).unwrap();
    (0..ni)
        .map(|i| interpolator.interpolate(timing_offset * i as f32 / (nxd as f32 - 1.0)))
        .collect::<Vec<f32>>()
}
