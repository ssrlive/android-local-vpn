use std::{collections::VecDeque, io::ErrorKind};

pub(crate) enum Buffers {
    Tcp(TcpBuffers),
    Udp(UdpBuffers),
}

impl Buffers {
    pub(crate) fn recv_data(&mut self, event: IncomingDataEvent<'_>) {
        match self {
            Buffers::Tcp(tcp_buf) => tcp_buf.recv_data(event),
            Buffers::Udp(udp_buf) => udp_buf.recv_data(event),
        }
    }

    pub(crate) fn consume_data<F>(&mut self, direction: OutgoingDirection, mut write_fn: F)
    where
        F: FnMut(&[u8]) -> crate::Result<usize>,
    {
        match self {
            Buffers::Tcp(tcp_buf) => {
                let buffer = tcp_buf.peek_data(&direction).to_vec();
                match write_fn(&buffer[..]) {
                    Ok(consumed) => {
                        tcp_buf.consume_data(&direction, consumed);
                    }
                    Err(error) => match error {
                        crate::Error::Io(err) => {
                            if err.kind() == ErrorKind::WouldBlock {
                            } else {
                                log::error!("failed to write tcp, direction: {:?}, error={:?}", direction, err);
                            }
                        }
                        crate::Error::TcpSend(err) => {
                            log::error!("failed to write tcp, direction: {:?}, error={:?}", direction, err);
                        }
                        _ => {
                            log::error!("failed to write tcp, direction: {:?}, error={:?}", direction, error);
                        }
                    },
                }
            }
            Buffers::Udp(udp_buf) => {
                let all_datagrams = udp_buf.peek_data(&direction);
                let mut consumed: usize = 0;
                // write udp packets one by one
                for datagram in all_datagrams {
                    if let Err(error) = write_fn(&datagram[..]) {
                        match error {
                            crate::Error::Io(err) => {
                                if err.kind() == ErrorKind::WouldBlock {
                                    break;
                                } else {
                                    log::error!("failed to write udp, direction: {:?}, error={:?}", direction, err);
                                }
                            }
                            crate::Error::UdpSend(err) => {
                                use ::smoltcp::socket::udp::SendError;
                                if err == SendError::BufferFull || err == SendError::Unaddressable {
                                    break;
                                } else {
                                    log::error!("failed to write udp, direciton: {:?}, error={:?}", direction, err);
                                }
                            }
                            _ => {
                                log::error!("failed to write udp, direction: {:?}, error={:?}", direction, error);
                            }
                        }
                    }
                    consumed += 1;
                }
                udp_buf.consume_data(&direction, consumed);
            }
        }
    }
}

pub(crate) struct TcpBuffers {
    client_buf: VecDeque<u8>,
    server_buf: VecDeque<u8>,
}

impl TcpBuffers {
    pub(crate) fn new() -> TcpBuffers {
        TcpBuffers {
            client_buf: Default::default(),
            server_buf: Default::default(),
        }
    }

    pub(crate) fn peek_data(&mut self, direction: &OutgoingDirection) -> &[u8] {
        let buffer = match direction {
            OutgoingDirection::ToServer => &mut self.server_buf,
            OutgoingDirection::ToClient => &mut self.client_buf,
        };
        buffer.make_contiguous()
    }

    pub(crate) fn consume_data(&mut self, direction: &OutgoingDirection, size: usize) {
        let buffer = match direction {
            OutgoingDirection::ToServer => &mut self.server_buf,
            OutgoingDirection::ToClient => &mut self.client_buf,
        };
        buffer.drain(0..size);
    }

    pub(crate) fn recv_data(&mut self, event: IncomingDataEvent<'_>) {
        match event.direction {
            IncomingDirection::FromServer => {
                self.client_buf.extend(event.buffer.iter());
            }
            IncomingDirection::FromClient => {
                self.server_buf.extend(event.buffer.iter());
            }
        }
    }
}

pub(crate) struct UdpBuffers {
    client_buf: VecDeque<Vec<u8>>,
    server_buf: VecDeque<Vec<u8>>,
}

impl UdpBuffers {
    pub(crate) fn new() -> UdpBuffers {
        UdpBuffers {
            client_buf: VecDeque::default(),
            server_buf: VecDeque::default(),
        }
    }

    pub(crate) fn peek_data(&mut self, direction: &OutgoingDirection) -> &[Vec<u8>] {
        let buffer = match direction {
            OutgoingDirection::ToServer => &mut self.server_buf,
            OutgoingDirection::ToClient => &mut self.client_buf,
        };
        buffer.make_contiguous()
    }

    pub(crate) fn consume_data(&mut self, direction: &OutgoingDirection, size: usize) {
        let buffer = match direction {
            OutgoingDirection::ToServer => &mut self.server_buf,
            OutgoingDirection::ToClient => &mut self.client_buf,
        };
        buffer.drain(0..size);
    }

    pub(crate) fn recv_data(&mut self, event: IncomingDataEvent<'_>) {
        match event.direction {
            IncomingDirection::FromServer => self.client_buf.push_back(event.buffer.to_vec()),
            IncomingDirection::FromClient => self.server_buf.push_back(event.buffer.to_vec()),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, PartialOrd, Ord, Hash)]
pub(crate) enum IncomingDirection {
    FromServer,
    FromClient,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, PartialOrd, Ord, Hash)]
pub(crate) enum OutgoingDirection {
    ToServer,
    ToClient,
}

pub(crate) struct DataEvent<'a, T> {
    pub direction: T,
    pub buffer: &'a [u8],
}

pub(crate) type IncomingDataEvent<'a> = DataEvent<'a, IncomingDirection>;
