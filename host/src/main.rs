use clap::Parser;
use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::os::unix::io::{AsRawFd, RawFd};

static OUT_INTERFACE: std::sync::OnceLock<CString> = std::sync::OnceLock::new();

/// Tunnel traffic through sockets.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the tun interface.
    #[arg(short, long)]
    tun: String,

    /// Name of the output interface.
    #[arg(short, long)]
    out: String,
}

#[cfg(target_os = "linux")]
fn main() {
    use env_logger::Env;
    use smoltcp::phy::{Medium, TunTapInterface};

    let environment = Env::default().default_filter_or("tuncore=info");
    env_logger::Builder::from_env(environment).init();

    let args = Args::parse();

    OUT_INTERFACE.set(CString::new(args.out).unwrap()).unwrap();

    tuncore::tun_callbacks::set_socket_created_callback(Some(on_socket_created));

    let tun_name = &args.tun;
    match TunTapInterface::new(tun_name, Medium::Ip) {
        Ok(tun) => {
            set_panic_handler();

            tuncore::tun::create();
            tuncore::tun::start(tun.as_raw_fd());

            /*
            println!("Press any key to exit");
            std::io::stdin().read_line(&mut String::new()).unwrap();
            // */

            let (tx, rx) = std::sync::mpsc::channel();
            let handle = ctrlc2::set_handler(move || {
                tx.send(()).expect("Could not send signal on channel.");
                true
            })
            .expect("Error setting Ctrl-C handler");
            println!("Press Ctrl-C to exit");
            rx.recv().expect("Could not receive from channel.");
            handle.join().unwrap();

            tuncore::tun::stop();
            tuncore::tun::destroy();

            remove_panic_handler();
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("failed to attach to tun {:?}; permission denied", tun_name);
        }
        Err(_) => {
            eprintln!("failed to attach to tun {:?}", tun_name);
        }
    }
}

#[cfg(target_os = "linux")]
fn on_socket_created(socket: RawFd) {
    bind_socket_to_interface(socket, OUT_INTERFACE.get().unwrap());
}

#[cfg(target_os = "linux")]
fn bind_socket_to_interface(socket: RawFd, interface: &CString) {
    let result = unsafe {
        libc::setsockopt(
            socket,
            libc::SOL_SOCKET,
            libc::SO_BINDTODEVICE,
            interface.as_ptr() as *const libc::c_void,
            std::mem::size_of::<CString>() as libc::socklen_t,
        )
    };
    if result == -1 {
        let error_code = unsafe { *libc::__errno_location() };
        let error: std::io::Result<libc::c_int> = Err(std::io::Error::from_raw_os_error(error_code));
        eprint!("failed to bind socket to interface, error={:?}", error);
    }
}

#[cfg(target_os = "linux")]
fn set_panic_handler() {
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("PANIC [{:?}]", panic_info);
    }));
}

#[cfg(target_os = "linux")]
fn remove_panic_handler() {
    let _ = std::panic::take_hook();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This program is only supported on Linux");
    OUT_INTERFACE.set(CString::new("dummy".to_string()).unwrap()).unwrap();
}
