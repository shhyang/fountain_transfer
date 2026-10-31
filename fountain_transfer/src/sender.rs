//! Send a file as fountain-coded UDP symbols.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use fountain_transfer_core::{TransferEncoder, TransferEncoderImpl};
use tokio::net::UdpSocket;
use tokio::time::sleep;

use crate::net::{NetError, send_message};
use crate::protocol::{packet_message, session_meta_from_spec};
use crate::session::SessionParams;

/// Sender pacing and repair-symbol settings.
#[derive(Debug, Clone)]
pub struct SendConfig {
    /// Repair symbols generated per round. Set to zero to send source symbols only.
    pub repair_count: usize,
    /// Number of repair rounds. Each round starts after the previous round's repair range.
    pub repair_rounds: usize,
    /// Delay inserted after each transmitted datagram.
    pub inter_packet_delay: Duration,
}

impl Default for SendConfig {
    fn default() -> Self {
        Self {
            repair_count: 256,
            repair_rounds: 1,
            inter_packet_delay: Duration::ZERO,
        }
    }
}

/// Read a file and transmit it to `dest`.
pub async fn send_file(
    path: &Path,
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
) -> Result<(), NetError> {
    let object = std::fs::read(path).map_err(NetError::Io)?;
    send_object(&object, dest, session, config).await
}

/// Transmit object bytes to `dest` using a new ephemeral UDP socket.
pub async fn send_object(
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
) -> Result<(), NetError> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    send_object_with_socket(&socket, object, dest, session, config).await
}

/// Transmit object bytes using the caller-provided UDP socket.
pub async fn send_object_with_socket(
    socket: &UdpSocket,
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
) -> Result<(), NetError> {
    let mut encoder = TransferEncoderImpl::new(session.spec.clone(), object)?;

    send_message(
        socket,
        dest,
        &session_meta_from_spec(session.session_id, &session.spec),
    )
    .await?;

    for packet in encoder.source_packets() {
        send_message(socket, dest, &packet_message(session.session_id, &packet)).await?;
        if !config.inter_packet_delay.is_zero() {
            sleep(config.inter_packet_delay).await;
        }
    }

    for round in 0..config.repair_rounds {
        let start = round * config.repair_count;
        for packet in encoder.repair_packets(start, config.repair_count) {
            send_message(socket, dest, &packet_message(session.session_id, &packet)).await?;
            if !config.inter_packet_delay.is_zero() {
                sleep(config.inter_packet_delay).await;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SendConfig;
    use std::time::Duration;

    #[test]
    fn defaults_match_cli_pacing_and_repair_settings() {
        let config = SendConfig::default();
        assert_eq!(config.repair_count, 256);
        assert_eq!(config.repair_rounds, 1);
        assert_eq!(config.inter_packet_delay, Duration::ZERO);
    }
}
