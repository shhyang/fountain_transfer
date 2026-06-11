//! Receive fountain-coded UDP symbols and recover the object.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use fountain_transfer_core::{DecodeStatus, TransferDecoder, TransferDecoderImpl};
use tokio::net::UdpSocket;

use crate::net::{recv_message_timeout, NetError};
use crate::protocol::{spec_from_session_meta, transfer_packet_from_message, WireMessage};
use crate::session::SessionParams;

#[derive(Debug, Clone)]
pub struct RecvConfig {
    pub recv_timeout: Duration,
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

pub struct RecvOutcome {
    pub object: Vec<u8>,
    pub packets_received: usize,
    pub peer: SocketAddr,
}

pub async fn receive_to_file(
    listen: &str,
    output: &Path,
    config: &RecvConfig,
) -> Result<RecvOutcome, NetError> {
    let outcome = receive_object(listen, config).await?;
    std::fs::write(output, &outcome.object).map_err(NetError::Io)?;
    Ok(outcome)
}

pub async fn receive_object(listen: &str, config: &RecvConfig) -> Result<RecvOutcome, NetError> {
    let socket = UdpSocket::bind(listen).await?;
    receive_object_with_socket(&socket, config).await
}

pub async fn receive_object_with_socket(
    socket: &UdpSocket,
    config: &RecvConfig,
) -> Result<RecvOutcome, NetError> {
    let mut session: Option<SessionParams> = None;
    let mut decoder: Option<TransferDecoderImpl> = None;
    let mut packets_received = 0usize;

    loop {
        let (message, peer) = recv_message_timeout(socket, config.recv_timeout).await?;

        match message {
            WireMessage::SessionMeta { session_id, .. } => {
                if let Some(expected) = config.expected_session_id
                    && session_id != expected
                {
                    continue;
                }
                let spec = spec_from_session_meta(&message)?;
                session = Some(SessionParams { session_id, spec: spec.clone() });
                decoder = Some(
                    TransferDecoderImpl::new(spec).map_err(|e| {
                        NetError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            e.to_string(),
                        ))
                    })?,
                );
            }
            WireMessage::Packet { session_id, .. } => {
                let Some(sess) = session.as_ref() else {
                    continue;
                };
                if sess.session_id != session_id {
                    continue;
                }
                if let Some(expected) = config.expected_session_id
                    && session_id != expected
                {
                    continue;
                }
                let packet = transfer_packet_from_message(
                    &message,
                    sess.session_id,
                    sess.spec.symbol_size_t,
                )?;
                let dec = decoder.as_mut().expect("decoder after meta");
                let status = dec.add_packet(&packet).map_err(|e| {
                    NetError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    ))
                })?;
                packets_received += 1;
                if status == DecodeStatus::Decoded {
                    let object = dec.recover_object().map_err(|e| {
                        NetError::Io(std::io::Error::other(e.to_string()))
                    })?;
                    return Ok(RecvOutcome {
                        object,
                        packets_received,
                        peer,
                    });
                }
            }
        }
    }
}
