//! Send a file as fountain-coded UDP symbols.

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use fountain_transfer_core::{TransferEncoder, TransferEncoderImpl};
use tokio::net::UdpSocket;
use tokio::time::sleep;

use crate::net::{send_message, NetError};
use crate::protocol::{packet_message, session_meta_from_spec};
use crate::session::SessionParams;

#[derive(Debug, Clone)]
pub struct SendConfig {
    pub repair_count: usize,
    pub repair_rounds: usize,
    pub inter_packet_delay: Duration,
}

impl Default for SendConfig {
    fn default() -> Self {
        Self {
            repair_count: 256,
            repair_rounds: 1,
            inter_packet_delay: Duration::from_millis(2),
        }
    }
}

pub async fn send_file(
    path: &Path,
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
) -> Result<(), NetError> {
    let object = std::fs::read(path).map_err(NetError::Io)?;
    send_object(&object, dest, session, config).await
}

pub async fn send_object(
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
) -> Result<(), NetError> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    send_object_with_socket(&socket, object, dest, session, config).await
}

pub async fn send_object_with_socket(
    socket: &UdpSocket,
    object: &[u8],
    dest: SocketAddr,
    session: &SessionParams,
    config: &SendConfig,
) -> Result<(), NetError> {
    let mut encoder =
        TransferEncoderImpl::new(session.spec.clone(), object).map_err(|e| {
            NetError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))
        })?;

    send_message(
        socket,
        dest,
        &session_meta_from_spec(session.session_id, &session.spec),
    )
    .await?;

    for packet in encoder.source_packets() {
        send_message(
            socket,
            dest,
            &packet_message(session.session_id, &packet),
        )
        .await?;
        if !config.inter_packet_delay.is_zero() {
            sleep(config.inter_packet_delay).await;
        }
    }

    for round in 0..config.repair_rounds {
        let start = round * config.repair_count;
        for packet in encoder.repair_packets(start, config.repair_count) {
            send_message(
                socket,
                dest,
                &packet_message(session.session_id, &packet),
            )
            .await?;
            if !config.inter_packet_delay.is_zero() {
                sleep(config.inter_packet_delay).await;
            }
        }
    }

    Ok(())
}
