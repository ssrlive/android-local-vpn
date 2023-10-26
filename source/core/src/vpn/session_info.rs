use smoltcp::wire::{IpProtocol, IpVersion, Ipv4Packet, Ipv6Packet, TcpPacket, UdpPacket};
use std::{fmt, hash::Hash, net::SocketAddr};

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, PartialOrd, Ord)]
pub(crate) struct SessionInfo {
    pub(crate) source: SocketAddr,
    pub(crate) destination: SocketAddr,
    pub(crate) ip_protocol: IpProtocol,
    pub(crate) ip_version: IpVersion,
}

impl SessionInfo {
    pub(crate) fn new(bytes: &Vec<u8>) -> Option<SessionInfo> {
        Self::new_ipv4(bytes).or_else(|| Self::new_ipv6(bytes)).or_else(|| {
            log::error!("failed to create session info, len={:?}", bytes.len(),);
            None
        })
    }

    fn new_ipv4(bytes: &Vec<u8>) -> Option<SessionInfo> {
        if let Ok(ip_packet) = Ipv4Packet::new_checked(&bytes) {
            match ip_packet.next_header() {
                IpProtocol::Tcp => {
                    let payload = ip_packet.payload();
                    let packet = TcpPacket::new_checked(payload).unwrap();
                    let source_ip: [u8; 4] = ip_packet.src_addr().as_bytes().try_into().unwrap();
                    let destination_ip: [u8; 4] = ip_packet.dst_addr().as_bytes().try_into().unwrap();
                    return Some(SessionInfo {
                        source: SocketAddr::from((source_ip, packet.src_port())),
                        destination: SocketAddr::from((destination_ip, packet.dst_port())),
                        ip_protocol: IpProtocol::Tcp,
                        ip_version: IpVersion::Ipv4,
                    });
                }
                IpProtocol::Udp => {
                    let payload = ip_packet.payload();
                    let packet = UdpPacket::new_checked(payload).unwrap();
                    let source_ip: [u8; 4] = ip_packet.src_addr().as_bytes().try_into().unwrap();
                    let destination_ip: [u8; 4] = ip_packet.dst_addr().as_bytes().try_into().unwrap();
                    return Some(SessionInfo {
                        source: SocketAddr::from((source_ip, packet.src_port())),
                        destination: SocketAddr::from((destination_ip, packet.dst_port())),
                        ip_protocol: IpProtocol::Udp,
                        ip_version: IpVersion::Ipv4,
                    });
                }
                _ => {
                    log::warn!("unsupported transport protocol, protocol=${:?}", ip_packet.next_header());
                    return None;
                }
            }
        }

        None
    }

    fn new_ipv6(bytes: &Vec<u8>) -> Option<SessionInfo> {
        if let Ok(ip_packet) = Ipv6Packet::new_checked(&bytes) {
            let protocol = ip_packet.next_header();
            match protocol {
                IpProtocol::Tcp => {
                    let payload = ip_packet.payload();
                    let packet = TcpPacket::new_checked(payload).unwrap();
                    let source_ip: [u8; 16] = ip_packet.src_addr().as_bytes().try_into().unwrap();
                    let destination_ip: [u8; 16] = ip_packet.dst_addr().as_bytes().try_into().unwrap();
                    return Some(SessionInfo {
                        source: SocketAddr::from((source_ip, packet.src_port())),
                        destination: SocketAddr::from((destination_ip, packet.dst_port())),
                        ip_protocol: IpProtocol::Tcp,
                        ip_version: IpVersion::Ipv6,
                    });
                }
                IpProtocol::Udp => {
                    let payload = ip_packet.payload();
                    let packet = UdpPacket::new_checked(payload).unwrap();
                    let source_ip: [u8; 16] = ip_packet.src_addr().as_bytes().try_into().unwrap();
                    let destination_ip: [u8; 16] = ip_packet.dst_addr().as_bytes().try_into().unwrap();
                    return Some(SessionInfo {
                        source: SocketAddr::from((source_ip, packet.src_port())),
                        destination: SocketAddr::from((destination_ip, packet.dst_port())),
                        ip_protocol: IpProtocol::Udp,
                        ip_version: IpVersion::Ipv6,
                    });
                }
                _ => {
                    log::warn!("unsupported transport protocol, protocol=${:?}", protocol);
                    return None;
                }
            }
        }

        None
    }
}

impl fmt::Display for SessionInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "[{:?}][{:?}]{}:{}->{}:{}",
            self.ip_version,
            self.ip_protocol,
            self.source.ip(),
            self.source.port(),
            self.destination.ip(),
            self.destination.port()
        )
    }
}
