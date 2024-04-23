use crossbeam_channel::unbounded;
use interp1d::Interp1d;
use modem::{
    uart::{UartRx, UartTx},
    v21::{V21RX, V21TX},
};
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, Normal, Uniform};
use rand_pcg;
use std::f32::consts::PI;

const BAUD_RATE: usize = 300;

#[test]
fn uart_trivial_48000() {
    test_uart(48000, false, false)
}

#[test]
fn uart_trivial_44100() {
    test_uart(44100, false, false)
}

#[test]
fn uart_unsync_48000() {
    test_uart(48000, false, true)
}

#[test]
fn uart_unsync_44100() {
    test_uart(44100, false, true)
}

#[test]
fn uart_noisy_48000() {
    test_uart(48000, true, false)
}

#[test]
fn uart_noisy_44100() {
    test_uart(44100, true, false)
}

#[test]
fn uart_noisy_unsync_48000() {
    test_uart(48000, true, true)
}

#[test]
fn uart_noisy_unsync_44100() {
    test_uart(44100, true, true)
}

#[test]
fn v21_sync_48000() {
    test_v21(48000, false)
}

#[test]
fn v21_sync_44100() {
    test_v21(44100, false)
}

#[test]
fn v21_unsync_48000() {
    test_v21(48000, true)
}

#[test]
fn v21_unsync_44100() {
    test_v21(44100, true)
}

fn test_uart(srate: usize, add_noise: bool, add_timing_offset: bool) {
    let samples_per_symbol = srate / BAUD_RATE;

    let (rx_sender, rx_receiver) = unbounded();

    let mut uart_tx = UartTx::new(samples_per_symbol);
    let mut uart_rx = UartRx::new(samples_per_symbol, rx_sender);

    let mut gen = rand_pcg::Pcg32::seed_from_u64(42);
    let d_idle_samples = Uniform::new(0, samples_per_symbol);
    let d_msg_bytes = Uniform::new(1, 100);
    let d_byte = Uniform::new(0, 255);
    let d_timing_offset = Uniform::new(0.98, 1.02);

    for iteration in 0..50 {
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

fn test_v21(srate: usize, add_timing_offset: bool) {
    const MAX_EBN0_DB: usize = 20;
    let mut ber_ebn0_db = vec![0.; MAX_EBN0_DB];
    for ebn0_db in 0..MAX_EBN0_DB {
        let ber = compute_v21_ber(srate, ebn0_db as f32, add_timing_offset);
        println!("EbN0 = {} dB, BER = {}", ebn0_db, ber);
        ber_ebn0_db[ebn0_db] = ber;
    }
    assert!(ber_ebn0_db[10] <= 1e-1);
    assert!(ber_ebn0_db[12] <= 1e-2);
    assert!(ber_ebn0_db[16] <= 1e-3);
    assert!(ber_ebn0_db[19] <= 1e-5);
}

fn compute_v21_ber(srate: usize, ebn0_db: f32, add_timing_offset: bool) -> f32 {
    0.5 * (compute_v21_ber_on_direction(srate, true, ebn0_db, add_timing_offset)
        + compute_v21_ber_on_direction(srate, false, ebn0_db, add_timing_offset))
}

fn compute_v21_ber_on_direction(
    srate: usize,
    tx_call: bool,
    ebn0_db: f32,
    add_timing_offset: bool,
) -> f32 {
    let samples_per_symbol = srate / BAUD_RATE;
    let sampling_period = 1. / srate as f32;

    let center_freq = if tx_call { 1080. } else { 1750. };
    let (tx_omega0, tx_omega1) = (
        2. * PI * (center_freq + 100.),
        2. * PI * (center_freq - 100.),
    );

    let mut gen = rand_pcg::Pcg32::seed_from_u64(42);
    let d_idle_samples = Uniform::new(2 * samples_per_symbol, 4 * samples_per_symbol);
    let d_msg_bytes = Uniform::new(1, 100);
    let d_byte = Uniform::new(0, 255);
    let d_timing_offset = Uniform::new(0.98, 1.02);

    let mut mean_ber = 0.;
    const NUM_ITERATIONS: usize = 50;

    for _ in 0..NUM_ITERATIONS {
        let (rx_sender, rx_receiver) = unbounded();

        let mut uart_tx = UartTx::new(samples_per_symbol);
        let mut uart_rx = UartRx::new(samples_per_symbol, rx_sender);
        let mut v21_tx = V21TX::new(sampling_period, tx_omega1, tx_omega0);
        let mut v21_rx = V21RX::new(sampling_period, samples_per_symbol, tx_omega1, tx_omega0);

        let idle_samples = d_idle_samples.sample(&mut gen);
        let idle_end = 2 * samples_per_symbol;
        let msg_bytes = d_msg_bytes.sample(&mut gen);
        let msg_samples = 10 * samples_per_symbol * msg_bytes;
        let n = idle_samples + msg_samples + idle_end;

        let mut uart_out = vec![0; n];
        let mut transmitted_samples = vec![0.0; n];
        uart_tx.get_samples(&mut uart_out[..idle_samples]);

        let orig_msg: Vec<u8> = d_byte.sample_iter(&mut gen).take(msg_bytes).collect();
        for b in &orig_msg {
            uart_tx.put_byte(*b);
        }
        uart_tx.get_samples(&mut uart_out[idle_samples..]);
        v21_tx.modulate(&uart_out, &mut transmitted_samples);

        let timing_offset = if add_timing_offset {
            d_timing_offset.sample(&mut gen)
        } else {
            1.0
        };

        let received_samples = awgn_channel_ebn0_db(
            &mut gen,
            samples_per_symbol,
            ebn0_db,
            timing_offset,
            &transmitted_samples,
        );

        let d_cut = Uniform::new(1, received_samples.len() - 1);
        let cut = d_cut.sample(&mut gen);

        let mut uart_in = vec![0; cut];
        v21_rx.demodulate(&received_samples[..cut], &mut uart_in);
        uart_rx.put_samples(&uart_in);

        let mut uart_in = vec![0; received_samples.len() - cut];
        v21_rx.demodulate(&received_samples[cut..], &mut uart_in);
        uart_rx.put_samples(&uart_in);

        let mut bit_errors = 0;
        let max_size = rx_receiver.len().max(msg_bytes);
        for i in 0..max_size {
            let a = rx_receiver.try_recv().unwrap_or(0);
            let b = if i < msg_bytes { orig_msg[i] } else { 0 };
            bit_errors += (a ^ b).count_ones();
        }

        let ber = bit_errors as f32 / (8. * max_size as f32);
        mean_ber += ber / NUM_ITERATIONS as f32;
    }

    mean_ber
}

fn awgn_channel_ebn0_db<R: Rng + ?Sized>(
    gen: &mut R,
    samples_per_symbol: usize,
    ebn0_db: f32,
    timing_offset: f32,
    samples: &[f32],
) -> Vec<f32> {
    // see https://www.mathworks.com/help/comm/ug/awgn-channel.html
    // in our case, Eb == Es, since we have one bit per symbol
    let snr_db = ebn0_db - 10. * (0.5 * samples_per_symbol as f32).log10();

    let s_db = 10. * signal_avg_power(samples).log10();
    let n_db = s_db - snr_db;
    let n = 10.0_f32.powf(n_db / 10.0);

    awgn_channel(gen, n.sqrt(), timing_offset, samples)
}

fn awgn_channel<R: Rng + ?Sized>(
    gen: &mut R,
    noise_amplitude: f32,
    timing_offset: f32,
    samples: &[f32],
) -> Vec<f32> {
    let d = Normal::new(0., noise_amplitude).unwrap();
    let yd = samples
        .iter()
        .map(|sample| sample + d.sample(gen))
        .collect::<Vec<f32>>();
    apply_timing_offset(timing_offset, &yd)
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

fn signal_avg_power(samples: &[f32]) -> f32 {
    let n = samples.len();
    samples
        .iter()
        .map(|sample| sample * sample / n as f32)
        .sum()
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
