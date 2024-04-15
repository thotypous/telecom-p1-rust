use anyhow;

#[cfg_attr(unix, path = "serial_linux.rs")]
#[cfg_attr(windows, path = "serial_windows.rs")]
mod serial;

fn main() -> anyhow::Result<()> {
    let mut serial = serial::Serial::open("", Box::new(|b| println!("received {}", b)))?;
    serial.event_loop()?;
    Ok(())
}
