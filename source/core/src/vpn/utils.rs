use smoltcp::wire::{IpProtocol, Ipv4Packet, TcpPacket, UdpPacket};

pub fn log_packet(message: &str, bytes: &Vec<u8>) {
    let result = Ipv4Packet::new_checked(&bytes);
    match result {
        Ok(ip_packet) => match ip_packet.next_header() {
            IpProtocol::Tcp => {
                let tcp_bytes = ip_packet.payload();
                let tcp_packet = TcpPacket::new_checked(tcp_bytes).unwrap();
                log::debug!(
                    "[{:?}] len={:?} tcp=[{}] tcp_len={:?} ip=[{}]",
                    message,
                    bytes.len(),
                    tcp_packet,
                    tcp_bytes.len(),
                    ip_packet
                );
            }
            IpProtocol::Udp => {
                let udp_bytes = ip_packet.payload();
                let udp_packet = UdpPacket::new_checked(udp_bytes).unwrap();
                log::debug!(
                    "[{:?}] len={:?} udp=[{}] udp_len={:?} ip=[{}]",
                    message,
                    bytes.len(),
                    udp_packet,
                    udp_bytes.len(),
                    ip_packet
                );
            }
            _ => {
                log::debug!("[{:?}] len={:?} ip=[{}]", message, bytes.len(), ip_packet);
            }
        },
        Err(error) => {
            log::error!("[{:?}] failed to log packet, error={:?}", message, error);
        }
    }
}
