use crate::vpn::{
    buffers::{Buffers, IncomingDataEvent, IncomingDirection, OutgoingDirection, TcpBuffers, UdpBuffers},
    mio_socket,
    session_info::SessionInfo,
    smoltcp_socket,
    vpn_device::VpnDevice,
};
use mio::{Poll, Token};
use smoltcp::{
    iface::{Config, Interface, SocketSet},
    time::Instant,
    wire::{HardwareAddress, IpAddress, IpCidr, IpProtocol, Ipv4Address},
};

pub(crate) struct Session<'a> {
    pub(crate) token: Token,
    smoltcp_socket: smoltcp_socket::Socket,
    mio_socket: mio_socket::Socket,
    buffers: Buffers,
    interface: Interface,
    sockets: SocketSet<'a>,
    device: VpnDevice,
    expiry: Option<::std::time::Instant>,
    session_info: SessionInfo,
    lifetime: ::std::time::Instant,
}

impl<'a> Session<'a> {
    pub(crate) fn new(session_info: &SessionInfo, poll: &mut Poll, token: Token) -> crate::Result<Session<'a>> {
        let mut device = VpnDevice::new();
        let mut sockets = SocketSet::new([]);

        let expiry = if session_info.ip_protocol == IpProtocol::Udp {
            Some(Self::generate_expiry_timestamp(crate::UDP_TIMEOUT))
        } else {
            None
        };

        let session = Session {
            smoltcp_socket: Self::create_smoltcp_socket(session_info, &mut sockets)?,
            mio_socket: Self::create_mio_socket(session_info, poll, token)?,
            token,
            buffers: Self::create_buffer(session_info.ip_protocol)?,
            interface: Self::create_interface(&mut device)?,
            sockets,
            device,
            expiry,
            session_info: *session_info,
            lifetime: ::std::time::Instant::now(),
        };

        Ok(session)
    }

    pub(crate) fn destroy(&mut self, poll: &mut Poll) -> crate::Result<()> {
        let mut smoltcp_socket = self.smoltcp_socket.get(&mut self.sockets)?;
        smoltcp_socket.close();

        let mio_socket = &mut self.mio_socket;
        if let Err(err) = mio_socket.deregister_poll(poll) {
            log::error!("failed to deregister socket from poll, error={:?}", err);
        }
        mio_socket.close();

        Ok(())
    }

    pub(crate) fn read_from_smoltcp(&mut self) -> crate::Result<()> {
        log::trace!("read from smoltcp, session={:?}", self.session_info);

        let mut data = [0_u8; crate::MAX_PACKET_SIZE];
        loop {
            let mut socket = self.smoltcp_socket.get(&mut self.sockets)?;
            if !socket.can_receive() {
                break;
            }
            let data_len = socket.receive(&mut data);
            if let Err(e) = data_len {
                log::error!("failed to receive from smoltcp socket, error={:?}", e);
                break;
            }
            let data_len = data_len?;
            let event = IncomingDataEvent {
                direction: IncomingDirection::FromClient,
                buffer: &data[..data_len],
            };
            self.buffers.store_data(event);
        }
        Ok(())
    }

    pub(crate) fn write_to_smoltcp(&mut self) -> crate::Result<()> {
        log::trace!("write to smoltcp, session={:?}", self.session_info);

        let mut socket = self.smoltcp_socket.get(&mut self.sockets)?;
        if socket.can_send() {
            self.buffers.consume_data_with_fn(OutgoingDirection::ToClient, |b| socket.send(b))?;
        }
        Ok(())
    }

    pub(crate) fn store_tun_data(&mut self, raw_ip_packet: Vec<u8>) {
        crate::vpn::utils::log_packet("out", &raw_ip_packet);
        self.device.store_data(raw_ip_packet);
    }

    pub(crate) fn write_to_tun(&mut self, tun: &mut impl std::io::Write) -> crate::Result<()> {
        log::trace!("write to tun, session={:?}", self.session_info);

        // cook the packets in smoltcp framework.
        if !self.interface.poll(Instant::now(), &mut self.device, &mut self.sockets) {
            log::trace!("no readiness of socket might have changed. {:?}", self.session_info);
        }

        // write the cooked data(raw IP packets) to tun.
        while let Some(bytes) = self.device.pop_data() {
            crate::vpn::utils::log_packet("in", &bytes);
            tun.write_all(&bytes[..])?;
        }

        Ok(())
    }

    pub(crate) fn read_from_server(&mut self, is_closed: &mut bool) -> crate::Result<()> {
        log::trace!("read from server, session={:?}", self.session_info);
        let mut read_seqs = Vec::new();
        let error = self.mio_socket.read(is_closed, |bytes| {
            read_seqs.push(bytes.to_vec());
            Ok(())
        });
        if let Err(error) = error {
            assert_ne!(error.kind(), std::io::ErrorKind::WouldBlock);
            if error.kind() != std::io::ErrorKind::ConnectionReset {
                log::error!("failed to read from tcp stream, error={:?}", error);
            }
        };

        // here we can hijeck the data from server to client

        for bytes in read_seqs {
            if !bytes.is_empty() {
                let event = IncomingDataEvent {
                    direction: IncomingDirection::FromServer,
                    buffer: &bytes[..],
                };
                self.buffers.store_data(event);
            }
        }
        Ok(())
    }

    pub(crate) fn write_to_server(&mut self) -> crate::Result<()> {
        log::trace!("write to server, session={:?}", self.session_info);

        // here we can hijeck the data from client to server

        /*
        if let Some(data) = self.buffers.peek_data(OutgoingDirection::ToServer) {
            if data.is_empty() {
                self.buffers.consume_data(OutgoingDirection::ToServer, 0);
                return Ok(());
            }
            let mut size = 0;
            match self.mio_socket.write(data) {
                Ok(len) => {
                    size = len;
                }
                Err(error) => {
                    if error.kind() != std::io::ErrorKind::WouldBlock {
                        log::error!("write to server, error={:?}", error);
                    }
                }
            }

            self.buffers.consume_data(OutgoingDirection::ToServer, size);
        }
        // */

        self.buffers
            .consume_data_with_fn(OutgoingDirection::ToServer, |b| self.mio_socket.write(b).map_err(|e| e.into()))?;
        Ok(())
    }

    pub(crate) fn update_expiry_timestamp(&mut self, force_set: bool) {
        self.lifetime = ::std::time::Instant::now();
        if force_set {
            self.expiry = Some(Self::generate_expiry_timestamp(crate::TCP_TIMEOUT));
        } else if let Some(expiry) = self.expiry.as_mut() {
            *expiry = Self::generate_expiry_timestamp(crate::UDP_TIMEOUT);
        }
    }

    pub(crate) fn is_expired(&self) -> bool {
        if self.session_info.ip_protocol == IpProtocol::Tcp && self.lifetime.elapsed().as_secs() >= crate::TCP_MAX_LIFETIME {
            // TCP session is expired if it's lifetime is greater than 2 hours.
            return true;
        }
        if let Some(expiry) = self.expiry {
            expiry <= ::std::time::Instant::now()
        } else {
            false
        }
    }

    fn create_smoltcp_socket(info: &SessionInfo, sockets: &mut SocketSet<'_>) -> crate::Result<smoltcp_socket::Socket> {
        smoltcp_socket::Socket::new(info.ip_protocol, info.source, info.destination, sockets)
    }

    fn create_mio_socket(info: &SessionInfo, poll: &mut Poll, token: Token) -> std::io::Result<mio_socket::Socket> {
        let mut mio_socket = mio_socket::Socket::new(info.ip_protocol, info.ip_version, info.destination)?;

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

    fn create_buffer(ip_protocol: IpProtocol) -> crate::Result<Buffers> {
        match ip_protocol {
            IpProtocol::Tcp => Ok(Buffers::Tcp(TcpBuffers::new())),
            IpProtocol::Udp => Ok(Buffers::Udp(UdpBuffers::new())),
            _ => Err(crate::Error::UnsupportedProtocol(ip_protocol)),
        }
    }

    fn generate_expiry_timestamp(secs: u64) -> ::std::time::Instant {
        ::std::time::Instant::now() + ::std::time::Duration::from_secs(secs)
    }
}
