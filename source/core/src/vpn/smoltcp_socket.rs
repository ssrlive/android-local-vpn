use crate::vpn::session_info::TransportProtocol;
use smoltcp::{
    iface::{SocketHandle, SocketSet},
    socket::{tcp, udp},
    wire::IpEndpoint,
};
use std::net::SocketAddr;

pub(crate) struct Socket {
    socket_handle: SocketHandle,
    transport_protocol: TransportProtocol,
    local_endpoint: IpEndpoint,
}

impl Socket {
    pub(crate) fn new(
        transport_protocol: TransportProtocol,
        local_address: SocketAddr,
        remote_address: SocketAddr,
        sockets: &mut SocketSet<'_>,
    ) -> Option<Socket> {
        let local_endpoint = IpEndpoint::from(local_address);

        let remote_endpoint = IpEndpoint::from(remote_address);

        let socket_handle = match transport_protocol {
            TransportProtocol::Tcp => {
                let socket = Self::create_tcp_socket(remote_endpoint).unwrap();
                sockets.add(socket)
            }
            TransportProtocol::Udp => {
                let socket = Self::create_udp_socket(remote_endpoint).unwrap();
                sockets.add(socket)
            }
        };

        let socket = Socket {
            socket_handle,
            transport_protocol,
            local_endpoint,
        };

        Some(socket)
    }

    fn create_tcp_socket<'a>(endpoint: IpEndpoint) -> Option<tcp::Socket<'a>> {
        let mut socket = tcp::Socket::new(tcp::SocketBuffer::new(vec![0; 1024 * 1024]), tcp::SocketBuffer::new(vec![0; 1024 * 1024]));

        if socket.listen(endpoint).is_err() {
            log::error!("failed to listen on socket, endpoint=[{}]", endpoint);
            return None;
        }

        socket.set_ack_delay(None);

        Some(socket)
    }

    fn create_udp_socket<'a>(endpoint: IpEndpoint) -> Option<udp::Socket<'a>> {
        let mut socket = udp::Socket::new(
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 1024 * 1024], vec![0; 1024 * 1024]),
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 1024 * 1024], vec![0; 1024 * 1024]),
        );

        if socket.bind(endpoint).is_err() {
            log::error!("failed to bind socket, endpoint=[{}]", endpoint);
            return None;
        }

        Some(socket)
    }

    pub(crate) fn get<'a, 'b>(&self, sockets: &'b mut SocketSet<'a>) -> SocketInstance<'a, 'b> {
        let socket = match self.transport_protocol {
            TransportProtocol::Tcp => {
                let socket = sockets.get_mut::<tcp::Socket>(self.socket_handle);
                SocketType::Tcp(socket)
            }
            TransportProtocol::Udp => {
                let socket = sockets.get_mut::<udp::Socket>(self.socket_handle);
                SocketType::Udp(socket, self.local_endpoint)
            }
        };

        SocketInstance { instance: socket }
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
