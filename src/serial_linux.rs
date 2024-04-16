use anyhow;
use nix;
use std::os::fd::{AsRawFd, OwnedFd};

pub struct Serial {
    rx: Box<dyn FnMut(u8)>,
    pty: OwnedFd,
}

impl Serial {
    pub fn open(_options: &str, rx: Box<dyn FnMut(u8)>) -> anyhow::Result<Self> {
        let res = nix::pty::openpty(None, None)?;
        let pty = res.master;

        let mut termios = nix::sys::termios::tcgetattr(&pty)?;
        nix::sys::termios::cfmakeraw(&mut termios);
        nix::sys::termios::cfsetspeed(&mut termios, nix::sys::termios::BaudRate::B115200)?;
        nix::sys::termios::tcsetattr(&pty, nix::sys::termios::SetArg::TCSANOW, &termios)?;

        let pty_name = nix::unistd::ttyname(&res.slave)?;
        eprintln!("criado porto serial em {}", pty_name.to_string_lossy());

        Ok(Self { rx, pty })
    }

    pub fn write(&mut self, byte: u8) -> anyhow::Result<()> {
        nix::unistd::write(&self.pty, &[byte])?;
        Ok(())
    }

    pub fn event_loop(&mut self) -> anyhow::Result<()> {
        loop {
            let mut buf: [u8; 1] = [0];
            let res = nix::unistd::read(self.pty.as_raw_fd(), &mut buf);
            match res {
                Ok(amount) => {
                    if amount == 1 {
                        (&mut self.rx)(buf[0]);
                    }
                }
                Err(nix::errno::Errno::EIO) => {
                    // ignora o EIO que acontece enquanto a outra ponta não conecta à pty
                    std::thread::sleep(std::time::Duration::from_millis(100))
                }
                Err(_) => {
                    res?;
                }
            }
        }
    }
}
