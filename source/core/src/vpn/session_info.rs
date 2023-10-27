use smoltcp::wire::{IpProtocol, IpVersion, Ipv4Packet, Ipv6Packet, TcpPacket, UdpPacket};
use std::{fmt, hash::Hash, net::SocketAddr};

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, PartialOrd, Ord)]
pub(crate) struct SessionInfo {
    pub(crate) ip_version: IpVersion,
    pub(crate) ip_protocol: IpProtocol,
    pub(crate) source: SocketAddr,
    pub(crate) destination: SocketAddr,
}

impl SessionInfo {
    pub(crate) fn new(bytes: &[u8]) -> crate::Result<SessionInfo> {
        Self::new_ipv4(bytes).or_else(|e| {
            if let crate::Error::UnsupportedProtocol(_) = e {
                Err(e)
            } else {
                Self::new_ipv6(bytes)
            }
        })
    }

    fn new_ipv4(bytes: &[u8]) -> crate::Result<SessionInfo> {
        if let Ok(ip_packet) = Ipv4Packet::new_checked(&bytes) {
            let protocol = ip_packet.next_header();
            match protocol {
                IpProtocol::Tcp => {
                    let payload = ip_packet.payload();
                    let packet = TcpPacket::new_checked(payload)?;
                    let source_ip: [u8; 4] = ip_packet.src_addr().as_bytes().try_into()?;
                    let destination_ip: [u8; 4] = ip_packet.dst_addr().as_bytes().try_into()?;
                    return Ok(SessionInfo {
                        source: SocketAddr::from((source_ip, packet.src_port())),
                        destination: SocketAddr::from((destination_ip, packet.dst_port())),
                        ip_protocol: IpProtocol::Tcp,
                        ip_version: IpVersion::Ipv4,
                    });
                }
                IpProtocol::Udp => {
                    let payload = ip_packet.payload();
                    let packet = UdpPacket::new_checked(payload)?;
                    let source_ip: [u8; 4] = ip_packet.src_addr().as_bytes().try_into()?;
                    let destination_ip: [u8; 4] = ip_packet.dst_addr().as_bytes().try_into()?;
                    return Ok(SessionInfo {
                        source: SocketAddr::from((source_ip, packet.src_port())),
                        destination: SocketAddr::from((destination_ip, packet.dst_port())),
                        ip_protocol: IpProtocol::Udp,
                        ip_version: IpVersion::Ipv4,
                    });
                }
                _ => {
                    return Err(crate::Error::UnsupportedProtocol(protocol));
                }
            }
        }
        let err = format!("failed to create session info, len={:?}", bytes.len());
        Err(crate::Error::from(err))
    }

    fn new_ipv6(bytes: &[u8]) -> crate::Result<SessionInfo> {
        if let Ok(ip_packet) = Ipv6Packet::new_checked(&bytes) {
            let protocol = ip_packet.next_header();
            match protocol {
                IpProtocol::Tcp => {
                    let payload = ip_packet.payload();
                    let packet = TcpPacket::new_checked(payload)?;
                    let source_ip: [u8; 16] = ip_packet.src_addr().as_bytes().try_into()?;
                    let destination_ip: [u8; 16] = ip_packet.dst_addr().as_bytes().try_into()?;
                    return Ok(SessionInfo {
                        source: SocketAddr::from((source_ip, packet.src_port())),
                        destination: SocketAddr::from((destination_ip, packet.dst_port())),
                        ip_protocol: IpProtocol::Tcp,
                        ip_version: IpVersion::Ipv6,
                    });
                }
                IpProtocol::Udp => {
                    let payload = ip_packet.payload();
                    let packet = UdpPacket::new_checked(payload)?;
                    let source_ip: [u8; 16] = ip_packet.src_addr().as_bytes().try_into()?;
                    let destination_ip: [u8; 16] = ip_packet.dst_addr().as_bytes().try_into()?;
                    return Ok(SessionInfo {
                        source: SocketAddr::from((source_ip, packet.src_port())),
                        destination: SocketAddr::from((destination_ip, packet.dst_port())),
                        ip_protocol: IpProtocol::Udp,
                        ip_version: IpVersion::Ipv6,
                    });
                }
                _ => {
                    return Err(crate::Error::UnsupportedProtocol(protocol));
                }
            }
        }
        let err = format!("failed to create session info, len={:?}", bytes.len());
        Err(crate::Error::from(err))
    }
}

impl fmt::Display for SessionInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "[{:?}][{:?}]{}->{}",
            self.ip_version, self.ip_protocol, self.source, self.destination
        )
    }
}
