use anyhow;
use serialport;
use std::sync::mpsc::{Receiver, Sender};

pub struct Serial {
    to_uart: Sender<u8>,
    port: Box<dyn serialport::SerialPort>,
}

impl Serial {
    pub fn open(
        options: &str,
        from_uart: Receiver<u8>,
        to_uart: Sender<u8>,
    ) -> anyhow::Result<Self> {
        let port = serialport::new(options, 115_200).open()?;
        {
            let mut port = port.try_clone().unwrap();
            std::thread::spawn(move || loop {
                let b = from_uart.recv().unwrap();
                port.write(&[b]).unwrap();
            });
        }
        Ok(Self { to_uart, port })
    }

    pub fn event_loop(&mut self) -> anyhow::Result<()> {
        loop {
            let mut buf: [u8; 1] = [0];
            let amount = self.port.read(&mut buf)?;
            if amount == 1 {
                self.to_uart.send(buf[0]).unwrap();
            }
        }
    }
}
