use anyhow;
use clap::Parser;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample,
    Sample,
    SizedSample,
    StreamConfig, //SupportedStreamConfig,
};
use std::sync::Mutex;
use std::{f32::consts::PI, sync::Arc};

#[cfg_attr(unix, path = "serial_linux.rs")]
#[cfg_attr(windows, path = "serial_windows.rs")]
mod serial;

mod uart;
mod v21;

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

    let rxdev = if opt.txdev == "default" {
        host.default_input_device()
    } else {
        host.output_devices()?
            .find(|x| x.name().map(|y| y == opt.txdev).unwrap_or(false))
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
    let txcfg = rxdev.default_output_config().unwrap();
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

    let baudrate = 300;
    let uart_tx = Arc::new(Mutex::new(uart::UartTx::new(
        txcfg.sample_rate().0 / baudrate,
    )));
    let uart_tx_ = uart_tx.clone();
    let mut serial = serial::Serial::open(
        &opt.serdev,
        Box::new(move |b| uart_tx_.lock().unwrap().put_byte(b)),
    )?;
    let txch = txcfg.channels() as usize;
    let mut v21_tx = v21::V21TX::new(1. / txcfg.sample_rate().0 as f32, txch, tx_omega1, tx_omega0);

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let uart_tx_ = uart_tx.clone();
    let txstream = txdev.build_output_stream(
        &txcfg.into(),
        move |audio_out: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let mut uart_out = vec![1; audio_out.len() / txch];
            uart_tx_.lock().unwrap().get_samples(&mut uart_out);
            v21_tx.modulate(&uart_out, audio_out)
        },
        err_fn,
        None,
    )?;
    txstream.play()?;

    serial.event_loop()?;

    Ok(())
}
