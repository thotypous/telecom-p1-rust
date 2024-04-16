use anyhow;
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Mutex;
use std::{f32::consts::PI, sync::Arc};

#[cfg_attr(unix, path = "serial_linux.rs")]
#[cfg_attr(windows, path = "serial_windows.rs")]
mod serial;

mod uart;
mod v21;

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
    let tx_channels = txcfg.channels() as usize;

    let uart_tx = Arc::new(Mutex::new(uart::UartTx::new(tx_samples_per_symbol)));
    let mut serial = {
        let uart_tx = uart_tx.clone();
        serial::Serial::open(
            &opt.serdev,
            Box::new(move |b| uart_tx.lock().unwrap().put_byte(b)),
        )?
    };
    let mut v21_tx = v21::V21TX::new(tx_speriod, tx_omega1, tx_omega0);

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let txstream = {
        let uart_tx = uart_tx.clone();
        txdev.build_output_stream(
            &txcfg.into(),
            move |audio_out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let bufsize = audio_out.len() / tx_channels;
                let mut uart_out = vec![1; bufsize];
                uart_tx.lock().unwrap().get_samples(&mut uart_out);
                let mut v21_out = vec![0.; bufsize];
                v21_tx.modulate(&uart_out, &mut v21_out);
                for (frame, sample) in audio_out.chunks_mut(tx_channels).zip(v21_out.iter()) {
                    for dest in frame.iter_mut() {
                        *dest = *sample; 
                    }
                }
            },
            err_fn,
            None,
        )?
    };
    txstream.play()?;

    serial.event_loop()?;

    Ok(())
}
