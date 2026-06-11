//! Session identifiers and transfer parameters shared by sender and receiver.

use fountain_transfer_core::{CodecKind, TransferError, TransferSpec};

#[derive(Debug, Clone)]
pub struct SessionParams {
    pub session_id: u64,
    pub spec: TransferSpec,
}

impl SessionParams {
    pub fn from_file_and_cli(
        session_id: u64,
        transfer_length_f: usize,
        symbol_size_t: usize,
        codec: CodecKind,
    ) -> Result<Self, TransferError> {
        Ok(Self {
            session_id,
            spec: TransferSpec::new(transfer_length_f, symbol_size_t, codec)?,
        })
    }
}

pub fn random_session_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    nanos as u64 ^ (nanos >> 32) as u64
}
