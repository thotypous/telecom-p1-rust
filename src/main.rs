#[cfg_attr(unix, path = "serial_linux.rs")]
#[cfg_attr(windows, path = "serial_windows.rs")]
mod serial;
mod uart;
mod v21;

use crate::serial::Serial;
use crate::uart::{UartRx, UartTx};
use crate::v21::{V21RX, V21TX};
use anyhow;
use clap::Parser;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    BuildStreamError, FromSample, SizedSample, Stream,
};
use crossbeam_channel::unbounded;
use std::sync::Mutex;
use std::{f32::consts::PI, sync::Arc};

const BAUD_RATE: usize = 300;

#[derive(Parser, Debug)]
#[command(version, about = "Dial-up modem", long_about = None)]
struct Opt {
    /// Answer side
    #[arg(short, long, default_value_t = false)]
    answer: bool,

    /// Audio device to use for RX
    #[arg(short, long, default_value_t = String::from("default"))]
    rxdev: String,

    /// Audio device to use for TX
    #[arg(short, long, default_value_t = String::from("default"))]
    txdev: String,

    /// Serial device (Windows-only)
    #[arg(short, long, default_value_t = String::from("\\\\.\\COM3"))]
    serdev: String,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let host = cpal::default_host();

    let rxdev = if opt.rxdev == "default" {
        host.default_input_device()
    } else {
        host.input_devices()?
            .find(|x| x.name().map(|y| y == opt.rxdev).unwrap_or(false))
    }
    .expect("failed to find RX device");
    let rxcfg = rxdev.default_input_config().unwrap();
    eprintln!("RX device: {}, config: {:?}", rxdev.name()?, rxcfg);

    let txdev = if opt.txdev == "default" {
        host.default_output_device()
    } else {
        host.output_devices()?
            .find(|x| x.name().map(|y| y == opt.txdev).unwrap_or(false))
    }
    .expect("failed to find TX device");
    let txcfg = txdev.default_output_config().unwrap();
    eprintln!("TX device: {}, config: {:?}", txdev.name()?, txcfg);

    let (tx_omega0, tx_omega1, rx_omega0, rx_omega1) = if opt.answer {
        (
            2. * PI * (1750. + 100.),
            2. * PI * (1750. - 100.),
            2. * PI * (1080. + 100.),
            2. * PI * (1080. - 100.),
        )
    } else {
        (
            2. * PI * (1080. + 100.),
            2. * PI * (1080. - 100.),
            2. * PI * (1750. + 100.),
            2. * PI * (1750. - 100.),
        )
    };

    let tx_srate = txcfg.sample_rate().0 as usize;
    assert!(
        tx_srate % BAUD_RATE == 0,
        "TX sampling rate {} is not a multiple of the baud rate {}",
        tx_srate,
        BAUD_RATE
    );
    let tx_samples_per_symbol = tx_srate / BAUD_RATE;
    let tx_speriod = 1. / tx_srate as f32;

    let (pty_to_uart_tx, uart_tx_from_pty) = unbounded();
    let (uart_rx_to_pty, pty_from_uart_rx) = unbounded();
    let mut serial = Serial::open(&opt.serdev, pty_from_uart_rx, pty_to_uart_tx)?;

    let uart_tx = Arc::new(Mutex::new(UartTx::new(tx_samples_per_symbol)));
    let v21_tx = V21TX::new(tx_speriod, tx_omega1, tx_omega0);

    {
        let uart_tx = uart_tx.clone();
        std::thread::spawn(move || loop {
            let b = uart_tx_from_pty.recv().unwrap();
            uart_tx.lock().unwrap().put_byte(b);
        });
    }

    let tx_stream = match txcfg.sample_format() {
        cpal::SampleFormat::I8 => tx_run::<i8>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        cpal::SampleFormat::I16 => tx_run::<i16>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        cpal::SampleFormat::I32 => tx_run::<i32>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        cpal::SampleFormat::I64 => tx_run::<i64>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        cpal::SampleFormat::U8 => tx_run::<u8>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        cpal::SampleFormat::U16 => tx_run::<u16>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        cpal::SampleFormat::U32 => tx_run::<u32>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        cpal::SampleFormat::U64 => tx_run::<u64>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        cpal::SampleFormat::F32 => tx_run::<f32>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        cpal::SampleFormat::F64 => tx_run::<f64>(&txdev, &txcfg.into(), uart_tx, v21_tx),
        sample_format => panic!("TX: Unsupported sample format '{sample_format}'"),
    }?;

    let rx_srate = rxcfg.sample_rate().0 as usize;
    assert!(
        rx_srate % BAUD_RATE == 0,
        "RX sampling rate {} is not a multiple of the baud rate {}",
        rx_srate,
        BAUD_RATE
    );
    let rx_samples_per_symbol = rx_srate / BAUD_RATE;
    let rx_speriod = 1. / rx_srate as f32;

    let uart_rx = UartRx::new(rx_samples_per_symbol, uart_rx_to_pty);
    let v21_rx = V21RX::new(rx_speriod, rx_samples_per_symbol, rx_omega1, rx_omega0);
    let rx_stream = match rxcfg.sample_format() {
        cpal::SampleFormat::I8 => rx_run::<i8>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        cpal::SampleFormat::I16 => rx_run::<i16>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        cpal::SampleFormat::I32 => rx_run::<i32>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        cpal::SampleFormat::I64 => rx_run::<i64>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        cpal::SampleFormat::U8 => rx_run::<u8>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        cpal::SampleFormat::U16 => rx_run::<u16>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        cpal::SampleFormat::U32 => rx_run::<u32>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        cpal::SampleFormat::U64 => rx_run::<u64>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        cpal::SampleFormat::F32 => rx_run::<f32>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        cpal::SampleFormat::F64 => rx_run::<f64>(&rxdev, &rxcfg.into(), uart_rx, v21_rx),
        sample_format => panic!("RX: Unsupported sample format '{sample_format}'"),
    }?;

    tx_stream.play()?;
    rx_stream.play()?;
    serial.event_loop()
}

pub fn tx_run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    uart_tx: Arc<Mutex<UartTx>>,
    mut v21_tx: V21TX,
) -> Result<Stream, BuildStreamError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;

    let err_fn = |err| eprintln!("TX stream error: {}", err);

    device.build_output_stream(
        config,
        move |audio_out: &mut [T], _: &cpal::OutputCallbackInfo| {
            let bufsize = audio_out.len() / channels;
            let mut uart_out = vec![1; bufsize];
            uart_tx.lock().unwrap().get_samples(&mut uart_out);

            let mut v21_out = vec![0.; bufsize];
            v21_tx.modulate(&uart_out, &mut v21_out);

            for (frame, sample) in audio_out.chunks_mut(channels).zip(v21_out.iter()) {
                for dest in frame.iter_mut() {
                    *dest = T::from_sample(*sample);
                }
            }
        },
        err_fn,
        None,
    )
}

pub fn rx_run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mut uart_rx: UartRx,
    mut v21_rx: V21RX,
) -> Result<Stream, BuildStreamError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;

    let err_fn = |err| eprintln!("RX stream error: {}", err);

    device.build_input_stream(
        config,
        move |audio_in: &[T], _: &cpal::InputCallbackInfo| {
            let bufsize = audio_in.len() / channels;
            let mut v21_in = vec![0.; bufsize];
            for (frame, dest) in audio_in.chunks(channels).zip(v21_in.iter_mut()) {
                *dest = frame.first().unwrap().to_sample::<f32>();
            }

            let mut uart_in = vec![1; bufsize];
            v21_rx.demodulate(&v21_in, &mut uart_in);

            uart_rx.put_samples(&uart_in);
        },
        err_fn,
        None,
    )
}
