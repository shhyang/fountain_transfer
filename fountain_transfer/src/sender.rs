//! Send a file as fountain-coded UDP symbols.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use fountain_transfer_core::{TransferEncoder, TransferEncoderImpl};
use tokio::net::UdpSocket;
use tokio::time::sleep;

use crate::net::{NetError, send_message, send_message_counted};
use crate::progress::{ProgressCallback, ProgressEvent};
use crate::protocol::{encode_message, packet_message, session_meta_from_spec};
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

/// Read a file and transmit it while reporting [`ProgressEvent`]s.
pub async fn send_file_with_progress(
    path: &Path,
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
    progress: &mut ProgressCallback<'_>,
) -> Result<(), NetError> {
    let object = std::fs::read(path).map_err(NetError::Io)?;
    send_object_with_progress(&object, dest, session, config, progress).await
}

/// Transmit object bytes to `dest` using a new ephemeral UDP socket.
pub async fn send_object(
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
) -> Result<(), NetError> {
    send_object_with_new_socket_impl(object, dest, session, config, None).await
}

/// Transmit object bytes while reporting [`ProgressEvent`]s.
pub async fn send_object_with_progress(
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
    progress: &mut ProgressCallback<'_>,
) -> Result<(), NetError> {
    send_object_with_new_socket_impl(object, dest, session, config, Some(progress)).await
}

async fn send_object_with_new_socket_impl(
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
    progress: Option<&mut ProgressCallback<'_>>,
) -> Result<(), NetError> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    send_object_with_socket_impl(&socket, object, dest, session, config, progress).await
}

/// Transmit object bytes using the caller-provided UDP socket.
pub async fn send_object_with_socket(
    socket: &UdpSocket,
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
) -> Result<(), NetError> {
    send_object_with_socket_impl(socket, object, dest, session, config, None).await
}

/// Transmit object bytes over `socket` while reporting [`ProgressEvent`]s.
pub async fn send_object_with_socket_with_progress(
    socket: &UdpSocket,
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
    progress: &mut ProgressCallback<'_>,
) -> Result<(), NetError> {
    send_object_with_socket_impl(socket, object, dest, session, config, Some(progress)).await
}

async fn send_object_with_socket_impl(
    socket: &UdpSocket,
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
    mut progress: Option<&mut ProgressCallback<'_>>,
) -> Result<(), NetError> {
    let mut encoder = TransferEncoderImpl::new(session.spec.clone(), object)?;
    let source_packets = encoder.source_packets();
    let mut repair_packets = Vec::new();
    for round in 0..config.repair_rounds {
        let start = round * config.repair_count;
        repair_packets.extend(encoder.repair_packets(start, config.repair_count));
    }

    let meta = session_meta_from_spec(session.session_id, &session.spec);
    let planned_bytes = if progress.is_some() {
        let mut bytes = encode_message(&meta)?.len();
        for packet in source_packets.iter().chain(repair_packets.iter()) {
            bytes += encode_message(&packet_message(session.session_id, packet))?.len();
        }
        bytes
    } else {
        0
    };

    if let Some(progress) = progress.as_deref_mut() {
        progress(ProgressEvent::SenderStarted {
            session_id: session.session_id,
            transfer_length_f: session.spec.transfer_length_f,
            symbol_size_t: session.spec.symbol_size_t,
            source_packets: source_packets.len(),
            repair_packets: repair_packets.len(),
            planned_bytes,
        });
    }

    let mut bytes_sent = if progress.is_some() {
        send_message_counted(socket, dest, &meta).await?
    } else {
        send_message(socket, dest, &meta).await?;
        0
    };

    for (index, packet) in source_packets.iter().enumerate() {
        let message = packet_message(session.session_id, packet);
        let bytes = if progress.is_some() {
            send_message_counted(socket, dest, &message).await?
        } else {
            send_message(socket, dest, &message).await?;
            0
        };
        bytes_sent += bytes;
        if let Some(progress) = progress.as_deref_mut() {
            progress(ProgressEvent::SenderSourcePacketSent {
                sent: index + 1,
                total: source_packets.len(),
                bytes_sent,
                planned_bytes,
            });
        }
        if !config.inter_packet_delay.is_zero() {
            sleep(config.inter_packet_delay).await;
        }
    }

    for (index, packet) in repair_packets.iter().enumerate() {
        let message = packet_message(session.session_id, packet);
        let bytes = if progress.is_some() {
            send_message_counted(socket, dest, &message).await?
        } else {
            send_message(socket, dest, &message).await?;
            0
        };
        bytes_sent += bytes;
        if let Some(progress) = progress.as_deref_mut() {
            progress(ProgressEvent::SenderRepairPacketSent {
                sent: index + 1,
                total: repair_packets.len(),
                bytes_sent,
                planned_bytes,
            });
        }
        if !config.inter_packet_delay.is_zero() {
            sleep(config.inter_packet_delay).await;
        }
    }

    if let Some(progress) = progress {
        progress(ProgressEvent::SenderFinished {
            source_packets: source_packets.len(),
            repair_packets: repair_packets.len(),
            bytes_sent,
            planned_bytes,
        });
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
