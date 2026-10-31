//! UDP send/receive helpers.

use std::net::SocketAddr;

use tokio::net::UdpSocket;
use tokio::time::{Duration, timeout};

use fountain_transfer_core::TransferError;

use crate::protocol::{ProtocolError, WireMessage, decode_message, encode_message};

const MAX_DATAGRAM: usize = 65_507;
const DEFAULT_RECV_TIMEOUT: Duration = Duration::from_secs(30);

/// Errors returned by the UDP transport and transfer helpers.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    /// Operating-system I/O failure.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Malformed or unsupported wire data.
    #[error("wire protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    /// Codec configuration or packet error.
    #[error(transparent)]
    Core(#[from] TransferError),
    /// No datagram arrived before the idle timeout.
    #[error("no datagram received within {0:?}")]
    Timeout(Duration),
    /// A serialized message does not fit in a UDP datagram.
    #[error(
        "encoded message is {0} bytes, exceeding the 65507-byte UDP limit; reduce --symbol-size"
    )]
    DatagramTooLarge(usize),
}

/// Bind a UDP socket to `addr`.
pub async fn bind_udp(addr: &str) -> Result<UdpSocket, NetError> {
    let socket = UdpSocket::bind(addr).await?;
    Ok(socket)
}

/// Encode and send one wire message to `addr`.
pub async fn send_message(
    socket: &UdpSocket,
    addr: SocketAddr,
    message: &WireMessage,
) -> Result<(), NetError> {
    let bytes = encode_message(message)?;
    if bytes.len() > MAX_DATAGRAM {
        return Err(NetError::DatagramTooLarge(bytes.len()));
    }
    socket.send_to(&bytes, addr).await?;
    Ok(())
}

/// Receive one wire message using the library's default idle timeout.
pub async fn recv_message(socket: &UdpSocket) -> Result<(WireMessage, SocketAddr), NetError> {
    recv_message_timeout(socket, DEFAULT_RECV_TIMEOUT).await
}

/// Receive one wire message, returning [`NetError::Timeout`] after `wait`.
pub async fn recv_message_timeout(
    socket: &UdpSocket,
    wait: Duration,
) -> Result<(WireMessage, SocketAddr), NetError> {
    let mut buf = vec![0u8; MAX_DATAGRAM];
    let (len, peer) = timeout(wait, socket.recv_from(&mut buf))
        .await
        .map_err(|_| NetError::Timeout(wait))??;
    let message = decode_message(&buf[..len])?;
    Ok((message, peer))
}
