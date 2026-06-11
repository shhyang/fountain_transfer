//! UDP send/receive helpers.

use std::net::SocketAddr;

use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};

use crate::protocol::{decode_message, encode_message, ProtocolError, WireMessage};

const MAX_DATAGRAM: usize = 65_507;
const DEFAULT_RECV_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Protocol(#[from] ProtocolError),
    #[error("receive timed out after {0:?}")]
    Timeout(Duration),
    #[error("datagram too large ({0} bytes)")]
    DatagramTooLarge(usize),
}

pub async fn bind_udp(addr: &str) -> Result<UdpSocket, NetError> {
    let socket = UdpSocket::bind(addr).await?;
    Ok(socket)
}

pub async fn send_message(socket: &UdpSocket, addr: SocketAddr, message: &WireMessage) -> Result<(), NetError> {
    let bytes = encode_message(message)?;
    if bytes.len() > MAX_DATAGRAM {
        return Err(NetError::DatagramTooLarge(bytes.len()));
    }
    socket.send_to(&bytes, addr).await?;
    Ok(())
}

pub async fn recv_message(socket: &UdpSocket) -> Result<(WireMessage, SocketAddr), NetError> {
    recv_message_timeout(socket, DEFAULT_RECV_TIMEOUT).await
}

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
