use std::ffi::CString;
#[cfg(target_os = "linux")]
use std::os::unix::io::{AsRawFd, RawFd};

static OUT_INTERFACE: std::sync::OnceLock<CString> = std::sync::OnceLock::new();

/// Tunnel traffic through sockets.
#[derive(::clap::Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the tun interface.
    #[arg(short, long)]
    tun: String,

    /// Name of the output interface.
    #[arg(short, long)]
    out: String,

    /// Verbosity level
    #[arg(short, long, value_name = "level", value_enum, default_value = "info")]
    verbosity: ArgVerbosity,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum ArgVerbosity {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use env_logger::Env;
    use smoltcp::phy::{Medium, TunTapInterface};

    let args = <Args as ::clap::Parser>::parse();

    let default = format!("tuncore={:?}", args.verbosity);
    let environment = Env::default().default_filter_or(default);
    env_logger::Builder::from_env(environment).init();

    OUT_INTERFACE.set(CString::new(args.out)?).map_err(|e| e.to_string_lossy().to_string())?;

    tuncore::tun_callbacks::set_socket_created_callback(Some(on_socket_created));

    let tun = TunTapInterface::new(&args.tun, Medium::Ip)?;

    set_panic_handler();

    tuncore::tun::create();
    tuncore::tun::start(tun.as_raw_fd());

    {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = ctrlc2::set_handler(move || {
            tx.send(()).expect("Could not send signal on channel.");
            true
        })?;
        println!("Press Ctrl-C to exit");
        rx.recv()?;
        handle.join().expect("Couldn't join on the associated thread");
    }

    tuncore::tun::stop();
    tuncore::tun::destroy();
    tuncore::tun_callbacks::set_socket_created_callback(None);

    remove_panic_handler();
    Ok(())
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
