mod error;
mod vpn;
pub use error::{Error, Result};

pub(crate) const MAX_PACKET_SIZE: usize = 0xffff;

pub mod tun {
    use crate::vpn::Vpn;
    use std::process;
    use std::sync::Mutex;

    lazy_static::lazy_static! {
        static ref VPN: Mutex<Option<Vpn>> = Mutex::new(None);
    }

    macro_rules! vpn {
        () => {
            VPN.lock().unwrap().as_mut().unwrap()
        };
    }

    pub fn create() {
        log::trace!("create, pid={}", process::id());
    }

    pub fn destroy() {
        log::trace!("destroy, pid={}", process::id());
    }

    pub fn start(file_descriptor: i32) {
        log::trace!("start, pid={}, fd={}", process::id(), file_descriptor);
        update_vpn(file_descriptor);
        vpn!().start().unwrap();
        log::trace!("started, pid={}, fd={}", process::id(), file_descriptor);
    }

    pub fn stop() {
        log::trace!("stop, pid={}", process::id());
        vpn!().stop().unwrap();
        log::trace!("stopped, pid={}", process::id());
    }

    fn update_vpn(file_descriptor: i32) {
        let mut vpn = VPN.lock().unwrap();
        *vpn = Some(Vpn::new(file_descriptor));
    }
}

#[cfg(target_family = "unix")]
pub mod tun_callbacks {

    use std::os::unix::io::RawFd;
    use std::sync::RwLock;

    lazy_static::lazy_static! {
        static ref CALLBACK: RwLock<fn(i32)> = RwLock::new(on_socket_created_stub);
    }

    pub fn set_socket_created_callback(callback: Option<fn(i32)>) {
        let mut current_callback = CALLBACK.write().unwrap();
        match callback {
            Some(callback) => *current_callback = callback,
            None => *current_callback = on_socket_created_stub,
        }
    }

    pub fn on_socket_created(socket: RawFd) {
        let callback = CALLBACK.read().unwrap();
        callback(socket);
    }

    fn on_socket_created_stub(_socket: RawFd) {}
}
