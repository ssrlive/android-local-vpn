use clap::Parser;
use env_logger::Env;
use smoltcp::phy::{Medium, TunTapInterface};
use std::ffi::CString;
use std::os::unix::io::AsRawFd;
use tuncore::tun_callbacks;

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

fn main() {
    let environment = Env::default().default_filter_or("info");
    env_logger::Builder::from_env(environment).init();

    let args = Args::parse();

    OUT_INTERFACE.set(CString::new(args.out).unwrap()).unwrap();

    tun_callbacks::set_socket_created_callback(Some(on_socket_created));

    let tun_name = &args.tun;
    match TunTapInterface::new(tun_name, Medium::Ip) {
        Ok(tun) => {
            set_panic_handler();

            tuncore::tun::create();
            tuncore::tun::start(tun.as_raw_fd());

            println!("Press any key to exit");
            std::io::stdin().read_line(&mut String::new()).unwrap();

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

fn on_socket_created(socket: i32) {
    bind_socket_to_interface(socket, OUT_INTERFACE.get().unwrap());
}

fn bind_socket_to_interface(socket: i32, interface: &CString) {
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

fn set_panic_handler() {
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("PANIC [{:?}]", panic_info);
    }));
}

fn remove_panic_handler() {
    let _ = std::panic::take_hook();
}
