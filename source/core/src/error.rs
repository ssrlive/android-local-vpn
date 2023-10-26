#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("smoltcp::socket::tcp::RecvError {0:?}")]
    TcpRecv(#[from] smoltcp::socket::tcp::RecvError),

    #[error("smoltcp::socket::tcp::SendError {0:?}")]
    TcpSend(#[from] smoltcp::socket::tcp::SendError),

    #[error("smoltcp::socket::udp::SendError {0:?}")]
    UdpSend(#[from] smoltcp::socket::udp::SendError),

    #[error("smoltcp::socket::udp::RecvError {0:?}")]
    UdpRecv(#[from] smoltcp::socket::udp::RecvError),

    #[error("smoltcp::wire::Error {0:?}")]
    Wire(#[from] smoltcp::wire::Error),

    #[error("smoltcp::wire::IpProtocol {0}")]
    UnsupportedProtocol(smoltcp::wire::IpProtocol),

    #[error("TryFromSliceError {0:?}")]
    TryFromSlice(#[from] std::array::TryFromSliceError),

    #[error("{0}")]
    String(String),
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Self::String(err.to_string())
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::String(err)
    }
}

impl From<&String> for Error {
    fn from(err: &String) -> Self {
        Self::String(err.to_string())
    }
}

impl From<Error> for std::io::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::Io(err) => err,
            _ => std::io::Error::new(std::io::ErrorKind::Other, err),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
