use std::sync::Mutex;

use anyhow;
use clap::Parser;

#[cfg_attr(unix, path = "serial_linux.rs")]
#[cfg_attr(windows, path = "serial_windows.rs")]
mod serial;

mod uart;

#[derive(Parser, Debug)]
#[command(version, about = "Dial-up modem", long_about = None)]
struct Opt {
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
    let uart_tx = Mutex::new(uart::UartTx::new(160));
    let mut serial = serial::Serial::open(&opt.serdev, Box::new(|b| uart_tx.lock().unwrap().put_byte(b)))?;
    serial.event_loop()?;
    Ok(())
}
