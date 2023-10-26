use smoltcp::{
    iface::{SocketHandle, SocketSet},
    socket::{tcp, udp},
    wire::{IpEndpoint, IpProtocol},
};
use std::net::SocketAddr;

pub(crate) struct Socket {
    socket_handle: SocketHandle,
    ip_protocol: IpProtocol,
    local_endpoint: IpEndpoint,
}

impl Socket {
    pub(crate) fn new(ip_protocol: IpProtocol, local_address: SocketAddr, remote_address: SocketAddr, sockets: &mut SocketSet<'_>) -> crate::Result<Socket> {
        let local_endpoint = IpEndpoint::from(local_address);

        let remote_endpoint = IpEndpoint::from(remote_address);

        let socket_handle = match ip_protocol {
            IpProtocol::Tcp => {
                let socket = Self::create_tcp_socket(remote_endpoint)?;
                sockets.add(socket)
            }
            IpProtocol::Udp => {
                let socket = Self::create_udp_socket(remote_endpoint)?;
                sockets.add(socket)
            }
            _ => return Err(crate::Error::UnsupportedProtocol(ip_protocol)),
        };

        let socket = Socket {
            socket_handle,
            ip_protocol,
            local_endpoint,
        };

        Ok(socket)
    }

    fn create_tcp_socket<'a>(endpoint: IpEndpoint) -> crate::Result<tcp::Socket<'a>> {
        let mut socket = tcp::Socket::new(tcp::SocketBuffer::new(vec![0; 1024 * 1024]), tcp::SocketBuffer::new(vec![0; 1024 * 1024]));
        socket.listen(endpoint)?;
        socket.set_ack_delay(None);
        Ok(socket)
    }

    fn create_udp_socket<'a>(endpoint: IpEndpoint) -> crate::Result<udp::Socket<'a>> {
        let mut socket = udp::Socket::new(
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 1024 * 1024], vec![0; 1024 * 1024]),
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 1024 * 1024], vec![0; 1024 * 1024]),
        );
        socket.bind(endpoint)?;
        Ok(socket)
    }

    pub(crate) fn get<'a, 'b>(&self, sockets: &'b mut SocketSet<'a>) -> crate::Result<SocketInstance<'a, 'b>> {
        let socket = match self.ip_protocol {
            IpProtocol::Tcp => {
                let socket = sockets.get_mut::<tcp::Socket>(self.socket_handle);
                SocketType::Tcp(socket)
            }
            IpProtocol::Udp => {
                let socket = sockets.get_mut::<udp::Socket>(self.socket_handle);
                SocketType::Udp(socket, self.local_endpoint)
            }
            _ => return Err(crate::Error::UnsupportedProtocol(self.ip_protocol)),
        };
        Ok(SocketInstance { instance: socket })
    }
}

pub(crate) struct SocketInstance<'a, 'b> {
    instance: SocketType<'a, 'b>,
}

enum SocketType<'a, 'b> {
    Tcp(&'b mut tcp::Socket<'a>),
    Udp(&'b mut udp::Socket<'a>, IpEndpoint),
}

impl<'a, 'b> SocketInstance<'a, 'b> {
    pub(crate) fn can_send(&self) -> bool {
        match &self.instance {
            SocketType::Tcp(socket) => socket.may_send(),
            SocketType::Udp(_, _) => true,
        }
    }

    pub(crate) fn send(&mut self, data: &[u8]) -> crate::Result<usize> {
        match &mut self.instance {
            SocketType::Tcp(socket) => Ok(socket.send_slice(data)?),
            SocketType::Udp(socket, local_endpoint) => Ok(socket.send_slice(data, *local_endpoint).and(Ok(data.len()))?),
        }
    }

    pub(crate) fn can_receive(&self) -> bool {
        match &self.instance {
            SocketType::Tcp(socket) => socket.can_recv(),
            SocketType::Udp(socket, _) => socket.can_recv(),
        }
    }

    pub(crate) fn receive(&'b mut self, data: &mut [u8]) -> crate::Result<usize> {
        match &mut self.instance {
            SocketType::Tcp(socket) => Ok(socket.recv_slice(data)?),
            SocketType::Udp(socket, _) => Ok(socket.recv_slice(data).map(|result| result.0)?),
        }
    }

    pub(crate) fn close(&mut self) {
        match &mut self.instance {
            SocketType::Tcp(socket) => socket.close(),
            SocketType::Udp(socket, _) => socket.close(),
        }
    }
}
