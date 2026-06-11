//! Wire serialization for UDP transfer (v0.1).

use fountain_transfer_core::{CodecKind, TransferError, TransferPacket, TransferSpec};
use serde::{Deserialize, Serialize};

pub const WIRE_VERSION: u8 = 1;

pub const CODEC_RAPTOR_Q: u8 = 0;
pub const CODEC_RAPTOR_10: u8 = 1;

/// Top-level UDP payload: session metadata once, then packets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireMessage {
    SessionMeta {
        version: u8,
        session_id: u64,
        codec: u8,
        transfer_length_f: u64,
        symbol_size_t: u16,
    },
    Packet {
        version: u8,
        session_id: u64,
        sbn: u32,
        esi: u32,
        symbol: Vec<u8>,
    },
}

/// Per-symbol frame (documented wire shape; use [`WireMessage::Packet`] on the wire).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireFrame {
    pub version: u8,
    pub session_id: u64,
    pub codec: u8,
    pub sbn: u32,
    pub esi: u32,
    pub symbol_size: u16,
    pub symbol: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("unsupported wire version {0}")]
    UnsupportedVersion(u8),
    #[error("unknown codec id {0}")]
    UnknownCodec(u8),
    #[error("symbol length {actual} does not match symbol_size {expected}")]
    SymbolLengthMismatch { expected: usize, actual: usize },
    #[error("bincode error: {0}")]
    Encode(#[from] bincode::Error),
    #[error("{0}")]
    TransferCore(#[from] TransferError),
}

pub fn encode_message(message: &WireMessage) -> Result<Vec<u8>, ProtocolError> {
    Ok(bincode::serialize(message)?)
}

pub fn decode_message(bytes: &[u8]) -> Result<WireMessage, ProtocolError> {
    Ok(bincode::deserialize(bytes)?)
}

pub fn session_meta_from_spec(session_id: u64, spec: &TransferSpec) -> WireMessage {
    let codec = match spec.codec_kind() {
        CodecKind::RaptorQ => CODEC_RAPTOR_Q,
        CodecKind::Raptor10 => CODEC_RAPTOR_10,
    };
    WireMessage::SessionMeta {
        version: WIRE_VERSION,
        session_id,
        codec,
        transfer_length_f: spec.transfer_length_f as u64,
        symbol_size_t: spec.symbol_size_t as u16,
    }
}

pub fn spec_from_session_meta(meta: &WireMessage) -> Result<TransferSpec, ProtocolError> {
    let WireMessage::SessionMeta {
        version,
        codec,
        transfer_length_f,
        symbol_size_t,
        ..
    } = meta
    else {
        return Err(ProtocolError::UnsupportedVersion(0));
    };
    if *version != WIRE_VERSION {
        return Err(ProtocolError::UnsupportedVersion(*version));
    }
    let transfer_length_f = *transfer_length_f as usize;
    let symbol_size_t = *symbol_size_t as usize;
    let kind = match *codec {
        CODEC_RAPTOR_Q => CodecKind::RaptorQ,
        CODEC_RAPTOR_10 => CodecKind::Raptor10,
        other => return Err(ProtocolError::UnknownCodec(other)),
    };
    Ok(TransferSpec::new(transfer_length_f, symbol_size_t, kind)?)
}

pub fn packet_message(session_id: u64, packet: &TransferPacket) -> WireMessage {
    WireMessage::Packet {
        version: WIRE_VERSION,
        session_id,
        sbn: packet.sbn,
        esi: packet.esi as u32,
        symbol: packet.symbol.clone(),
    }
}

pub fn transfer_packet_from_message(
    message: &WireMessage,
    expected_session: u64,
    symbol_size: usize,
) -> Result<TransferPacket, ProtocolError> {
    let WireMessage::Packet {
        version,
        session_id,
        sbn,
        esi,
        symbol,
    } = message
    else {
        return Err(ProtocolError::UnsupportedVersion(0));
    };
    if *version != WIRE_VERSION {
        return Err(ProtocolError::UnsupportedVersion(*version));
    }
    if *session_id != expected_session {
        return Err(ProtocolError::UnsupportedVersion(0));
    }
    if symbol.len() != symbol_size {
        return Err(ProtocolError::SymbolLengthMismatch {
            expected: symbol_size,
            actual: symbol.len(),
        });
    }
    Ok(TransferPacket {
        sbn: *sbn,
        esi: *esi as usize,
        symbol: symbol.clone(),
    })
}

pub fn codec_kind_from_cli(name: &str) -> Result<CodecKind, String> {
    match name.to_ascii_lowercase().as_str() {
        "raptorq" | "raptor-q" | "rfc6330" => Ok(CodecKind::RaptorQ),
        "raptor10" | "raptor-10" | "rfc5053" => Ok(CodecKind::Raptor10),
        other => Err(format!(
            "unknown codec {other:?}; use raptorq or raptor10"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_message_roundtrip() {
        let meta = WireMessage::SessionMeta {
            version: WIRE_VERSION,
            session_id: 42,
            codec: CODEC_RAPTOR_Q,
            transfer_length_f: 199,
            symbol_size_t: 10,
        };
        let bytes = encode_message(&meta).expect("encode");
        let decoded = decode_message(&bytes).expect("decode");
        assert_eq!(decoded, meta);

        let pkt = WireMessage::Packet {
            version: WIRE_VERSION,
            session_id: 42,
            sbn: 0,
            esi: 3,
            symbol: vec![7u8; 10],
        };
        let bytes = encode_message(&pkt).expect("encode");
        assert_eq!(decode_message(&bytes).expect("decode"), pkt);
    }

    #[test]
    #[cfg(feature = "raptor-q")]
    fn spec_from_meta_roundtrip_raptor_q() {
        let spec = TransferSpec::new(199, 10, CodecKind::RaptorQ).expect("spec");
        let meta = session_meta_from_spec(99, &spec);
        let restored = spec_from_session_meta(&meta).expect("spec");
        assert_eq!(restored.transfer_length_f, spec.transfer_length_f);
        assert_eq!(restored.codec_kind(), CodecKind::RaptorQ);
    }

    #[test]
    #[cfg(feature = "raptor-10")]
    fn spec_from_meta_roundtrip_raptor_10() {
        let spec = TransferSpec::new(77, 8, CodecKind::Raptor10).expect("spec");
        let meta = session_meta_from_spec(99, &spec);
        let restored = spec_from_session_meta(&meta).expect("spec");
        assert_eq!(restored.transfer_length_f, spec.transfer_length_f);
        assert_eq!(restored.codec_kind(), CodecKind::Raptor10);
    }
}
