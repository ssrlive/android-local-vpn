use smoltcp::{
    phy::{DeviceCapabilities, Medium},
    time::Instant,
};
use std::collections::VecDeque;

#[derive(Debug)]
pub(crate) struct VpnDevice {
    rx_queue: VecDeque<Vec<u8>>,
    tx_queue: VecDeque<Vec<u8>>,
}

impl VpnDevice {
    pub(crate) fn new() -> VpnDevice {
        VpnDevice {
            rx_queue: VecDeque::new(),
            tx_queue: VecDeque::new(),
        }
    }

    pub(crate) fn receive_data(&mut self, bytes: Vec<u8>) {
        self.rx_queue.push_back(bytes);
    }

    pub(crate) fn distribute_data(&mut self) -> Option<Vec<u8>> {
        self.tx_queue.pop_front()
    }
}

impl ::smoltcp::phy::Device for VpnDevice {
    type RxToken<'a> = RxToken where Self: 'a;
    type TxToken<'a> = TxToken<'a> where Self: 'a;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut default = DeviceCapabilities::default();
        default.max_transmission_unit = crate::MAX_PACKET_SIZE;
        default.medium = Medium::Ip;
        default
    }

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        self.rx_queue.pop_front().map(move |buffer| {
            let rx = RxToken { buffer };
            let tx = TxToken { queue: &mut self.tx_queue };
            (rx, tx)
        })
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(TxToken { queue: &mut self.tx_queue })
    }
}

pub(crate) struct RxToken {
    buffer: Vec<u8>,
}

impl ::smoltcp::phy::RxToken for RxToken {
    fn consume<R, F>(mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        f(&mut self.buffer)
    }
}

pub(crate) struct TxToken<'a> {
    queue: &'a mut VecDeque<Vec<u8>>,
}

impl<'a> ::smoltcp::phy::TxToken for TxToken<'a> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = vec![0; len];
        let result = f(&mut buffer);
        self.queue.push_back(buffer);
        result
    }
}
