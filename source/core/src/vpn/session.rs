use crate::vpn::{
    buffers::{Buffers, TcpBuffers, UdpBuffers},
    mio_socket::Socket as MioSocket,
    session_info::SessionInfo,
    smoltcp_socket::Socket as SmoltcpSocket,
    vpn_device::VpnDevice,
};
use mio::{Poll, Token};
use smoltcp::{
    iface::{Config, Interface, SocketSet},
    time::Instant,
    wire::{HardwareAddress, IpAddress, IpCidr, IpProtocol, Ipv4Address},
};

pub(crate) struct Session<'a> {
    pub(crate) smoltcp_socket: SmoltcpSocket,
    pub(crate) mio_socket: MioSocket,
    pub(crate) token: Token,
    pub(crate) buffers: Buffers,
    pub(crate) interface: Interface,
    pub(crate) sockets: SocketSet<'a>,
    pub(crate) device: VpnDevice,
}

impl<'a> Session<'a> {
    pub(crate) fn new(session_info: &SessionInfo, poll: &mut Poll, token: Token) -> crate::Result<Session<'a>> {
        let mut device = VpnDevice::new();
        let mut sockets = SocketSet::new([]);

        let session = Session {
            smoltcp_socket: Self::create_smoltcp_socket(session_info, &mut sockets)?,
            mio_socket: Self::create_mio_socket(session_info, poll, token)?,
            token,
            buffers: Self::create_buffer(session_info)?,
            interface: Self::create_interface(&mut device)?,
            sockets,
            device,
        };

        Ok(session)
    }

    fn create_smoltcp_socket(info: &SessionInfo, sockets: &mut SocketSet<'_>) -> crate::Result<SmoltcpSocket> {
        SmoltcpSocket::new(info.ip_protocol, info.source, info.destination, sockets)
    }

    fn create_mio_socket(info: &SessionInfo, poll: &mut Poll, token: Token) -> std::io::Result<MioSocket> {
        let mut mio_socket = MioSocket::new(info.ip_protocol, info.ip_version, info.destination)?;

        if let Err(error) = mio_socket.register_poll(poll, token) {
            log::error!("failed to register poll, error={:?}", error);
            return Err(error);
        }

        Ok(mio_socket)
    }

    fn create_interface<D>(device: &mut D) -> crate::Result<Interface>
    where
        D: ::smoltcp::phy::Device + ?Sized,
    {
        let default_gateway_ipv4 = Ipv4Address::new(0, 0, 0, 1);
        let config = Config::new(HardwareAddress::Ip);

        let mut interface = Interface::new(config, device, Instant::now());
        interface.set_any_ip(true);
        interface.update_ip_addrs(|ip_addrs| {
            ip_addrs.push(IpCidr::new(IpAddress::v4(0, 0, 0, 1), 0)).unwrap();
        });
        interface.routes_mut().add_default_ipv4_route(default_gateway_ipv4)?;

        Ok(interface)
    }

    fn create_buffer(session_info: &SessionInfo) -> crate::Result<Buffers> {
        match session_info.ip_protocol {
            IpProtocol::Tcp => Ok(Buffers::Tcp(TcpBuffers::new())),
            IpProtocol::Udp => Ok(Buffers::Udp(UdpBuffers::new())),
            _ => Err(crate::Error::UnsupportedProtocol(session_info.ip_protocol)),
        }
    }
}
