#[cfg_attr(unix, path = "serial_linux.rs")]
#[cfg_attr(windows, path = "serial_windows.rs")]
mod serial;
mod uart;
mod v21;

use anyhow;
use clap::Parser;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait}, BuildStreamError, FromSample, SizedSample, Stream
};
use serial::Serial;
use std::sync::Mutex;
use std::{f32::consts::PI, sync::Arc};
use uart::UartTx;
use v21::V21TX;

const BAUD_RATE: u32 = 300;

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

    let tx_srate = txcfg.sample_rate().0;
    assert!(
        tx_srate % BAUD_RATE == 0,
        "TX sampling rate {} is not a multiple of the baud rate {}",
        tx_srate,
        BAUD_RATE
    );
    let tx_samples_per_symbol = txcfg.sample_rate().0 / BAUD_RATE;
    let tx_speriod = 1. / tx_srate as f32;

    let uart_tx = Arc::new(Mutex::new(UartTx::new(tx_samples_per_symbol)));
    let mut serial = {
        let uart_tx = uart_tx.clone();
        Serial::open(
            &opt.serdev,
            Box::new(move |b| uart_tx.lock().unwrap().put_byte(b)),
        )?
    };
    let v21_tx = V21TX::new(tx_speriod, tx_omega1, tx_omega0);
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

    tx_stream.play()?;
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
