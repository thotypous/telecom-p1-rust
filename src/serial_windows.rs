use anyhow;
use crossbeam_channel::{Receiver, Sender};
use std::{mem::zeroed, ptr::null_mut};
use winapi::{
    shared::{minwindef::DWORD, ntdef::HANDLE, winerror::ERROR_IO_PENDING},
    um::{
        commapi::{GetCommState, SetCommState, SetCommTimeouts},
        errhandlingapi::GetLastError,
        fileapi::{CreateFileW, ReadFile, WriteFile, OPEN_EXISTING},
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        ioapiset::GetOverlappedResult,
        minwinbase::OVERLAPPED,
        synchapi::{CreateEventA, WaitForSingleObject},
        winbase::{
            COMMTIMEOUTS, DCB, DTR_CONTROL_ENABLE, FILE_FLAG_OVERLAPPED, INFINITE, NOPARITY,
            ONESTOPBIT, RTS_CONTROL_ENABLE, WAIT_OBJECT_0,
        },
        winnt::{GENERIC_READ, GENERIC_WRITE},
    },
};

struct SendPtr<T>(pub *mut T);
unsafe impl<T> Send for SendPtr<T> {}
unsafe impl<T> Sync for SendPtr<T> {}

pub struct Serial {
    to_uart: Sender<u8>,
    h_comm: HANDLE,
}

impl Serial {
    pub fn open(
        options: &str,
        from_uart: Receiver<u8>,
        to_uart: Sender<u8>,
    ) -> anyhow::Result<Self> {
        unsafe {
            let h_comm = CreateFileW(
                options
                    .encode_utf16()
                    .chain([0])
                    .collect::<Vec<_>>()
                    .as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                null_mut(),
            );
            assert!(
                h_comm != INVALID_HANDLE_VALUE,
                "serial: error opening port: {}",
                GetLastError()
            );

            let mut dcb: DCB = zeroed();
            assert!(
                GetCommState(h_comm, &mut dcb) != 0,
                "serial: error on GetCommState: {}",
                GetLastError()
            );

            dcb.BaudRate = 115_200;
            dcb.ByteSize = 8;
            dcb.StopBits = ONESTOPBIT;
            dcb.Parity = NOPARITY;
            dcb.set_fBinary(1);
            dcb.set_fDtrControl(DTR_CONTROL_ENABLE);
            dcb.set_fRtsControl(RTS_CONTROL_ENABLE);
            dcb.set_fDsrSensitivity(0);
            dcb.set_fTXContinueOnXoff(0);
            dcb.set_fOutX(0);
            dcb.set_fInX(0);
            dcb.set_fErrorChar(0);
            dcb.set_fNull(0);
            dcb.set_fAbortOnError(0);
            dcb.set_fOutxCtsFlow(0);
            dcb.set_fOutxDsrFlow(0);

            assert!(
                SetCommState(h_comm, &mut dcb) != 0,
                "serial: error on SetCommState: {}",
                GetLastError()
            );

            let mut timeouts = COMMTIMEOUTS {
                ReadIntervalTimeout: 1,
                ReadTotalTimeoutMultiplier: 0,
                ReadTotalTimeoutConstant: 0,
                WriteTotalTimeoutMultiplier: 0,
                WriteTotalTimeoutConstant: 0,
            };

            assert!(
                SetCommTimeouts(h_comm, &mut timeouts) != 0,
                "serial: error on SetCommTimeouts: {}",
                GetLastError()
            );

            let h_comm_send = SendPtr(h_comm);
            std::thread::spawn(move || loop {
                let _ = &h_comm_send;
                let byte = from_uart.recv().unwrap();

                let mut os_write: OVERLAPPED = zeroed();
                let mut dw_written: DWORD = 0;

                os_write.hEvent = CreateEventA(null_mut(), 1, 0, null_mut());
                if os_write.hEvent.is_null() {
                    eprintln!(
                        "serial write: error creating overlapped event: {}",
                        GetLastError()
                    );
                    continue;
                }

                let res = WriteFile(
                    h_comm_send.0,
                    [byte].as_ptr().cast(),
                    1,
                    &mut dw_written,
                    &mut os_write,
                );
                if res == 0 {
                    if GetLastError() != ERROR_IO_PENDING {
                        eprintln!("serial write: error on WriteFile: {}", GetLastError());
                    } else {
                        match WaitForSingleObject(os_write.hEvent, INFINITE) {
                            WAIT_OBJECT_0 => {
                                if GetOverlappedResult(
                                    h_comm_send.0,
                                    &mut os_write,
                                    &mut dw_written,
                                    0,
                                ) == 0
                                {
                                    eprintln!(
                                        "serial write: error on GetOverlappedResult: {}",
                                        GetLastError()
                                    );
                                }
                            }
                            _ => {
                                eprintln!(
                                    "serial write: error on WaitForSingleObject: {}",
                                    GetLastError()
                                );
                            }
                        }
                    }
                }

                CloseHandle(os_write.hEvent);
            });

            Ok(Self { to_uart, h_comm })
        }
    }

    pub fn event_loop(&mut self) -> anyhow::Result<()> {
        unsafe {
            let mut dw_read: DWORD = 0;
            let mut os_reader: OVERLAPPED = zeroed();
            let mut f_waiting_on_read: bool = false;

            os_reader.hEvent = CreateEventA(null_mut(), 1, 0, null_mut());
            assert!(
                !os_reader.hEvent.is_null(),
                "serial read: error creating overlapped event: {}",
                GetLastError()
            );

            loop {
                let mut buf: [u8; 1024] = zeroed();

                if !f_waiting_on_read {
                    if ReadFile(
                        self.h_comm,
                        buf.as_mut_ptr().cast(),
                        buf.len() as u32,
                        &mut dw_read,
                        &mut os_reader,
                    ) == 0
                    {
                        if GetLastError() != ERROR_IO_PENDING {
                            eprintln!("serial read: error on ReadFile: {}", GetLastError());
                        } else {
                            f_waiting_on_read = true;
                        }
                    } else {
                        for i in 0..dw_read {
                            self.to_uart.send(buf[i as usize]).unwrap();
                        }
                    }
                }

                if f_waiting_on_read {
                    match WaitForSingleObject(os_reader.hEvent, INFINITE) {
                        WAIT_OBJECT_0 => {
                            if GetOverlappedResult(self.h_comm, &mut os_reader, &mut dw_read, 0)
                                == 0
                            {
                                eprintln!(
                                    "serial read: error on GetOverlappedResult: {}",
                                    GetLastError()
                                );
                            } else {
                                for i in 0..dw_read {
                                    self.to_uart.send(buf[i as usize]).unwrap();
                                }
                            }
                            f_waiting_on_read = false;
                        }
                        _ => {
                            eprintln!(
                                "serial read: error on WaitForSingleObject: {}",
                                GetLastError()
                            );
                        }
                    }
                }
            }
        }
    }
}
