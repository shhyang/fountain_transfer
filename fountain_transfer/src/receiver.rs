//! Receive fountain-coded UDP symbols and recover the object.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use fountain_transfer_core::{DecodeStatus, TransferDecoder, TransferDecoderImpl};
use tokio::net::UdpSocket;

use crate::net::{NetError, recv_datagram_timeout};
use crate::progress::{IgnoredDatagramReason, ProgressCallback, ProgressEvent, decode_percent};
use crate::protocol::{
    ProtocolError, WireMessage, decode_message, spec_from_session_meta,
    transfer_packet_from_message,
};
use crate::session::SessionParams;

/// Receiver idle timeout and optional session filtering.
#[derive(Debug, Clone)]
pub struct RecvConfig {
    /// Maximum idle time while waiting for the next datagram.
    pub recv_timeout: Duration,
    /// When set, ignore sessions with a different identifier.
    ///
    /// When unset, the first session that supplies valid metadata is kept until
    /// it completes or times out. Foreign-session packets and metadata are then
    /// ignored rather than switching the active transfer.
    pub expected_session_id: Option<u64>,
}

impl Default for RecvConfig {
    fn default() -> Self {
        Self {
            recv_timeout: Duration::from_secs(60),
            expected_session_id: None,
        }
    }
}

/// Recovered object and summary details from a completed transfer.
pub struct RecvOutcome {
    /// Recovered, unpadded object bytes.
    pub object: Vec<u8>,
    /// Number of encoded symbols accepted by the decoder.
    pub packets_received: usize,
    /// Peer that supplied the packet which completed decoding.
    pub peer: SocketAddr,
    /// Local receiver address.
    pub listen_addr: SocketAddr,
}

/// Receive an object and write the completed result to `output`.
pub async fn receive_to_file(
    listen: &str,
    output: &Path,
    config: &RecvConfig,
) -> Result<RecvOutcome, NetError> {
    let outcome = receive_object(listen, config).await?;
    std::fs::write(output, &outcome.object).map_err(NetError::Io)?;
    Ok(outcome)
}

/// Receive an object and write it to `output` while reporting [`ProgressEvent`]s.
pub async fn receive_to_file_with_progress(
    listen: &str,
    output: &Path,
    config: &RecvConfig,
    progress: &mut ProgressCallback<'_>,
) -> Result<RecvOutcome, NetError> {
    let outcome = receive_object_with_progress(listen, config, progress).await?;
    std::fs::write(output, &outcome.object).map_err(NetError::Io)?;
    Ok(outcome)
}

/// Bind to `listen` and receive an object.
pub async fn receive_object(listen: &str, config: &RecvConfig) -> Result<RecvOutcome, NetError> {
    let socket = UdpSocket::bind(listen).await?;
    receive_object_with_socket(&socket, config).await
}

/// Bind to `listen`, receive an object, and report [`ProgressEvent`]s.
pub async fn receive_object_with_progress(
    listen: &str,
    config: &RecvConfig,
    progress: &mut ProgressCallback<'_>,
) -> Result<RecvOutcome, NetError> {
    let socket = UdpSocket::bind(listen).await?;
    receive_object_with_socket_with_progress(&socket, config, progress).await
}

/// Receive an object using the caller-provided UDP socket.
pub async fn receive_object_with_socket(
    socket: &UdpSocket,
    config: &RecvConfig,
) -> Result<RecvOutcome, NetError> {
    receive_object_with_socket_impl(socket, config, None).await
}

/// Receive an object over `socket` while reporting [`ProgressEvent`]s.
pub async fn receive_object_with_socket_with_progress(
    socket: &UdpSocket,
    config: &RecvConfig,
    progress: &mut ProgressCallback<'_>,
) -> Result<RecvOutcome, NetError> {
    receive_object_with_socket_impl(socket, config, Some(progress)).await
}

async fn receive_object_with_socket_impl(
    socket: &UdpSocket,
    config: &RecvConfig,
    mut progress: Option<&mut ProgressCallback<'_>>,
) -> Result<RecvOutcome, NetError> {
    let listen_addr = socket.local_addr().map_err(NetError::Io)?;
    let mut session: Option<SessionParams> = None;
    let mut decoder: Option<TransferDecoderImpl> = None;
    let mut packets_received = 0usize;
    let mut bytes_received = 0usize;
    let mut ignored_datagrams = 0usize;

    loop {
        let (datagram, peer) = recv_datagram_timeout(socket, config.recv_timeout).await?;
        let message = match decode_message(&datagram) {
            Ok(message) => message,
            Err(_) => {
                emit_ignored(
                    &mut progress,
                    &mut ignored_datagrams,
                    IgnoredDatagramReason::Malformed,
                );
                continue;
            }
        };

        match message {
            WireMessage::SessionMeta { session_id, .. } => {
                if let Some(expected) = config.expected_session_id
                    && session_id != expected
                {
                    emit_ignored(
                        &mut progress,
                        &mut ignored_datagrams,
                        IgnoredDatagramReason::ForeignSession,
                    );
                    continue;
                }
                if let Some(active) = session.as_ref()
                    && active.session_id != session_id
                {
                    emit_ignored(
                        &mut progress,
                        &mut ignored_datagrams,
                        IgnoredDatagramReason::ForeignSession,
                    );
                    continue;
                }
                let spec = match spec_from_session_meta(&message) {
                    Ok(spec) => spec,
                    Err(error) => {
                        emit_ignored(
                            &mut progress,
                            &mut ignored_datagrams,
                            reason_for_protocol_error(&error),
                        );
                        continue;
                    }
                };
                let new_decoder = match TransferDecoderImpl::new(spec.clone()) {
                    Ok(decoder) => decoder,
                    Err(_) => {
                        emit_ignored(
                            &mut progress,
                            &mut ignored_datagrams,
                            IgnoredDatagramReason::InvalidSessionMetadata,
                        );
                        continue;
                    }
                };

                // A repeated metadata frame with the active session id is an
                // intentional retransmission/restart. Discard all decoder state
                // and counters so a complete retransmission can recover.
                session = Some(SessionParams {
                    session_id,
                    spec: spec.clone(),
                });
                decoder = Some(new_decoder);
                packets_received = 0;
                bytes_received = 0;
                if let Some(progress) = progress.as_deref_mut() {
                    progress(ProgressEvent::ReceiverSessionEstablished {
                        session_id,
                        transfer_length_f: spec.transfer_length_f,
                        symbol_size_t: spec.symbol_size_t,
                        expected_source_symbols: expected_source_symbols(&spec),
                    });
                }
            }
            WireMessage::Packet { session_id, .. } => {
                let Some(sess) = session.as_ref() else {
                    emit_ignored(
                        &mut progress,
                        &mut ignored_datagrams,
                        IgnoredDatagramReason::BeforeSession,
                    );
                    continue;
                };
                if sess.session_id != session_id {
                    emit_ignored(
                        &mut progress,
                        &mut ignored_datagrams,
                        IgnoredDatagramReason::ForeignSession,
                    );
                    continue;
                }
                if let Some(expected) = config.expected_session_id
                    && session_id != expected
                {
                    emit_ignored(
                        &mut progress,
                        &mut ignored_datagrams,
                        IgnoredDatagramReason::ForeignSession,
                    );
                    continue;
                }
                let packet = match transfer_packet_from_message(
                    &message,
                    sess.session_id,
                    sess.spec.symbol_size_t,
                ) {
                    Ok(packet) => packet,
                    Err(error) => {
                        emit_ignored(
                            &mut progress,
                            &mut ignored_datagrams,
                            reason_for_protocol_error(&error),
                        );
                        continue;
                    }
                };
                let dec = decoder.as_mut().expect("decoder after meta");
                let status = match dec.add_packet(&packet) {
                    Ok(status) => status,
                    Err(_) => {
                        emit_ignored(
                            &mut progress,
                            &mut ignored_datagrams,
                            IgnoredDatagramReason::InvalidPacket,
                        );
                        continue;
                    }
                };
                packets_received += 1;
                bytes_received = bytes_received.saturating_add(packet.symbol.len());
                let expected = expected_source_symbols(&sess.spec);
                let percent =
                    decode_percent(packets_received, expected, status == DecodeStatus::Decoded);
                if let Some(progress) = progress.as_deref_mut() {
                    progress(ProgressEvent::ReceiverPacketAccepted {
                        packets_accepted: packets_received,
                        bytes_accepted: bytes_received,
                        expected_source_symbols: expected,
                        decode_percent: percent,
                    });
                }
                if status == DecodeStatus::Decoded {
                    let object = dec.recover_object()?;
                    if let Some(progress) = progress.as_deref_mut() {
                        progress(ProgressEvent::ReceiverDecodeComplete {
                            packets_accepted: packets_received,
                            bytes_accepted: bytes_received,
                            decode_percent: 100,
                        });
                    }
                    return Ok(RecvOutcome {
                        object,
                        packets_received,
                        peer,
                        listen_addr,
                    });
                }
            }
        }
    }
}

fn expected_source_symbols(spec: &fountain_transfer_core::TransferSpec) -> usize {
    spec.transfer_length_f.div_ceil(spec.symbol_size_t)
}

fn reason_for_protocol_error(error: &ProtocolError) -> IgnoredDatagramReason {
    match error {
        ProtocolError::UnsupportedVersion(_) => IgnoredDatagramReason::UnsupportedVersion,
        ProtocolError::UnknownCodec(_) => IgnoredDatagramReason::UnknownCodec,
        ProtocolError::SymbolLengthMismatch { .. } => IgnoredDatagramReason::InvalidPacket,
        ProtocolError::TransferCore(_) => IgnoredDatagramReason::InvalidSessionMetadata,
        ProtocolError::Encode(_) => IgnoredDatagramReason::Malformed,
    }
}

fn emit_ignored(
    progress: &mut Option<&mut ProgressCallback<'_>>,
    ignored_datagrams: &mut usize,
    reason: IgnoredDatagramReason,
) {
    *ignored_datagrams += 1;
    if let Some(progress) = progress.as_deref_mut() {
        progress(ProgressEvent::ReceiverDatagramIgnored {
            ignored_datagrams: *ignored_datagrams,
            reason,
        });
    }
}
