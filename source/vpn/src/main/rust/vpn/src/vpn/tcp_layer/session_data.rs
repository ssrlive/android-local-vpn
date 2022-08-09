// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <https://unlicense.org>

use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Token};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::io::Read;
use std::io::Result;
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;

const EVENT_CAPACITY: usize = 16;

pub struct SessionData {
    poll: Poll,
    socket: Option<Socket>,
    events: Events,
}

impl SessionData {
    pub fn new() -> SessionData {
        SessionData {
            poll: Poll::new().unwrap(),
            socket: None,
            events: Events::with_capacity(EVENT_CAPACITY),
        }
    }

    pub fn connect_stream(&mut self, ip: [u8; 4], port: u16) {
        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)).unwrap();

        let raw_fd = socket.as_raw_fd();
        let is_socket_protected = socket_protector!().protect_socket(raw_fd);
        log::trace!(
            "finished protecting socket, is_socket_protected={:?}",
            is_socket_protected
        );

        self.poll
            .registry()
            .register(&mut SourceFd(&raw_fd), Token(0), Interest::READABLE)
            .unwrap();

        let remote_address = SockAddr::from(SocketAddr::from((ip, port)));

        log::trace!(
            "attempting to connect to remote host, ip={:?}, port={:?}, remote_address=[{:?}]",
            ip,
            port,
            remote_address
        );

        let result = socket.connect(&remote_address);
        match result {
            Ok(_) => {
                log::trace!(
                    "successfully connected to remote host, ip={:?}, port={:?}, remote_address=[{:?}]",
                    ip,
                    port,
                    remote_address
                );
                socket.set_nonblocking(true).unwrap();
            }
            Err(error_code) => {
                log::error!(
                    "failed to connect to remote host, error_code={:?}, ip={:?}, port={:?}, remote_address=[{:?}]",
                    error_code,
                    ip,
                    port,
                    remote_address
                );
            }
        }

        self.socket = Some(socket);
    }

    pub fn is_data_available(&mut self) -> bool {
        let timeout = Some(std::time::Duration::from_millis(0));
        let result = self.poll.poll(&mut self.events, timeout);
        if let Ok(_) = result {
            return self.events.iter().count() > 0;
        } else {
            return false;
        };
    }

    pub fn send_data(&mut self, bytes: &Vec<u8>) -> Result<usize> {
        let bytes_as_array = &bytes[..];
        let result = self.socket.as_ref().unwrap().send(bytes_as_array);
        if let Ok(size) = result {
            log::trace!(
                "sent data to socket, size={:?}, data={:?}",
                size,
                bytes_as_array
            );
        }
        return result;
    }

    pub fn read_data(&mut self) -> Vec<u8> {
        let buffer_size = 1024;
        let mut request_buffer: Vec<u8> = vec![];
        if let Some(socket) = &mut self.socket {
            loop {
                log::trace!("attempting to read data from tcp socket");
                let mut buffer = vec![0; buffer_size];
                match socket.read(&mut buffer) {
                    Ok(read_size) => {
                        log::trace!("read data from tcp socket, size={:?}", read_size);
                        if read_size <= 0 {
                            log::trace!("no more data read from tcp socket");
                            break;
                        } else {
                            unsafe {
                                buffer.set_len(read_size);
                            }
                            request_buffer.append(&mut buffer);
                        }
                    }
                    Err(error) => {
                        log::error!("read data from tcp socket failed, error={:?}", error);
                        break;
                    }
                }
            }
        } else {
            log::error!("read data from tcp socket failed; socket does not exist");
        }
        log::trace!(
            "finished reading data from tcp socket, count={:?}",
            request_buffer.len()
        );
        return request_buffer;
    }
}