use anyhow;
use serialport;

pub struct Serial {
    rx: Box<dyn FnMut(u8)>,
    port: Box<dyn serialport::SerialPort>,
}

impl Serial {
    pub fn open(options: &str, rx: Box<dyn FnMut(u8)>) -> anyhow::Result<Self> {
        let port = serialport::new(options, 115_200).open()?;
        Ok(Serial { rx, port })
    }

    pub fn write(&mut self, byte: u8) -> anyhow::Result<()> {
        self.port.write(&[byte])?;
        Ok(())
    }

    pub fn event_loop(&mut self) -> anyhow::Result<()> {
        loop {
            let mut buf: [u8; 1] = [0];
            let amount = self.port.read(&mut buf)?;
            if amount == 1 {
                (&mut self.rx)(buf[0]);
            }
        }
    }
}
