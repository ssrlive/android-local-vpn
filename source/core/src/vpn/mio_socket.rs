use crate::tun_callbacks::on_socket_created;
use mio::{
    net::{TcpStream, UdpSocket},
    Interest, Poll, Token,
};
use smoltcp::wire::{IpProtocol, IpVersion};
use std::{
    io::{ErrorKind, Result},
    net::{Shutdown, SocketAddr},
    os::unix::io::{AsRawFd, FromRawFd},
};

pub(crate) struct Socket {
    _socket: socket2::Socket, // Need to retain so socket does not get closed.
    connection: Connection,
}

enum Connection {
    Tcp(TcpStream),
    Udp(UdpSocket),
}

impl Socket {
    pub(crate) fn new(ip_protocol: IpProtocol, ip_version: IpVersion, remote_address: SocketAddr) -> Option<Socket> {
        let socket = Self::create_socket(&ip_protocol, &ip_version);

        on_socket_created(socket.as_raw_fd());

        let socket_address = socket2::SockAddr::from(remote_address);

        log::debug!("connecting to host, address={:?}", remote_address);

        match socket.connect(&socket_address) {
            Ok(_) => {
                log::debug!("connected to host, address={:?}", remote_address);
            }
            Err(error) => {
                if error.kind() == ErrorKind::WouldBlock || error.raw_os_error() == Some(libc::EINPROGRESS) {
                    // do nothing.
                } else {
                    log::error!("failed to connect to host, error={:?} address={:?}", error, remote_address);
                    return None;
                }
            }
        }

        let connection = Self::create_connection(&ip_protocol, &socket);

        Some(Socket { _socket: socket, connection })
    }

    pub(crate) fn register_poll(&mut self, poll: &mut Poll, token: Token) -> std::io::Result<()> {
        match &mut self.connection {
            Connection::Tcp(connection) => {
                let interests = Interest::READABLE | Interest::WRITABLE;
                poll.registry().register(connection, token, interests)
            }
            Connection::Udp(connection) => {
                let interests = Interest::READABLE;
                poll.registry().register(connection, token, interests)
            }
        }
    }

    pub(crate) fn deregister_poll(&mut self, poll: &mut Poll) -> std::io::Result<()> {
        match &mut self.connection {
            Connection::Tcp(connection) => poll.registry().deregister(connection),
            Connection::Udp(connection) => poll.registry().deregister(connection),
        }
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> Result<usize> {
        match &mut self.connection {
            Connection::Tcp(connection) => connection.write(bytes),
            Connection::Udp(connection) => connection.write(bytes),
        }
    }

    pub(crate) fn read(&mut self) -> Result<(Vec<Vec<u8>>, bool)> {
        match &mut self.connection {
            Connection::Tcp(connection) => Self::read_all(connection),
            Connection::Udp(connection) => Self::read_all(connection),
        }
    }

    pub(crate) fn close(&self) {
        match &self.connection {
            Connection::Tcp(connection) => {
                if let Err(error) = connection.shutdown(Shutdown::Both) {
                    log::trace!("failed to shutdown tcp stream, error={:?}", error);
                }
            }
            Connection::Udp(_) => {
                // UDP connections do not require to be closed.
            }
        }
    }

    fn create_socket(ip_protocol: &IpProtocol, ip_version: &IpVersion) -> socket2::Socket {
        let domain = match ip_version {
            IpVersion::Ipv4 => socket2::Domain::IPV4,
            IpVersion::Ipv6 => socket2::Domain::IPV6,
        };

        let protocol = match ip_protocol {
            IpProtocol::Tcp => socket2::Protocol::TCP,
            IpProtocol::Udp => socket2::Protocol::UDP,
            _ => panic!("unsupported transport protocol"),
        };

        let socket_type = match ip_protocol {
            IpProtocol::Tcp => socket2::Type::STREAM,
            IpProtocol::Udp => socket2::Type::DGRAM,
            _ => panic!("unsupported transport protocol"),
        };

        let socket = socket2::Socket::new(domain, socket_type, Some(protocol)).unwrap();

        socket.set_nonblocking(true).unwrap();

        socket
    }

    fn create_connection(ip_protocol: &IpProtocol, socket: &socket2::Socket) -> Connection {
        match ip_protocol {
            IpProtocol::Tcp => {
                let tcp_stream = unsafe { TcpStream::from_raw_fd(socket.as_raw_fd()) };
                Connection::Tcp(tcp_stream)
            }
            IpProtocol::Udp => {
                let udp_socket = unsafe { UdpSocket::from_raw_fd(socket.as_raw_fd()) };
                Connection::Udp(udp_socket)
            }
            _ => panic!("unsupported transport protocol"),
        }
    }

    fn read_all<R>(reader: &mut R) -> Result<(Vec<Vec<u8>>, bool)>
    where
        R: Read,
    {
        let mut bytes: Vec<Vec<u8>> = Vec::new();
        let mut buffer = [0; 1 << 16]; // maximum UDP packet size
        let mut is_closed = false;
        loop {
            match reader.read(&mut buffer[..]) {
                Ok(count) => {
                    if count == 0 {
                        is_closed = true;
                        break;
                    }
                    // bytes.extend_from_slice(&buffer[..count]);
                    let data = buffer[..count].to_vec();
                    bytes.push(data)
                }
                Err(error_code) => {
                    if error_code.kind() == ErrorKind::WouldBlock {
                        break;
                    } else {
                        return Err(error_code);
                    }
                }
            }
        }
        Ok((bytes, is_closed))
    }
}

trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
}

impl Read for mio::net::UdpSocket {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.recv(buf)
    }
}

impl Read for mio::net::TcpStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        <mio::net::TcpStream as std::io::Read>::read(self, buf)
    }
}

trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
}

impl Write for mio::net::UdpSocket {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.send(buf)
    }
}

impl Write for mio::net::TcpStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        <mio::net::TcpStream as std::io::Write>::write(self, buf)
    }
}
