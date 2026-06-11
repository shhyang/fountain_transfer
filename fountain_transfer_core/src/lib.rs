//! Dual-codec transfer facade over RaptorQ and Raptor10.
//!
//! See the [crate README](../README.md) for usage, API tables, and v1 limitations.
//!
//! # Features
//!
//! - **`raptor-q`** (default) — RFC 6330 via `raptor_q`; enables [`CodecConfig::RaptorQ`].
//! - **`raptor-10`** (default) — RFC 5053 via `raptor_10`; enables [`CodecConfig::Raptor10`].
//!
//! Build with only one codec, e.g. `cargo build -p fountain_transfer_core --no-default-features --features raptor-q`.
//!
//! # Main types
//!
//! - [`TransferSpec`] / [`CodecConfig`] — object length, symbol size, codec choice
//! - [`TransferPacket`] — RFC-style `{ sbn, esi, symbol }`
//! - [`TransferEncoder`] / [`TransferDecoder`] — traits implemented by [`TransferEncoderImpl`] / [`TransferDecoderImpl`]
//! - [`StorageManager`] / [`SlabStorageManager`] — optional `fountain_operators` integration
//!
//! RaptorQ LDPC and maximum degree are fixed inside this crate ([`codec_config_from_kind`]); integrators need not configure `raptor_q` codec internals.
//!
//! Re-exported for integrators: [`DecodeStatus`].

pub use fountain_engine::types::DecodeStatus;

#[cfg(feature = "raptor-q")]
mod rfc6330_codec;
#[cfg(feature = "raptor-q")]
pub mod rfc6330_layout;
#[cfg(feature = "raptor-q")]
mod rfc6330_mapping;
#[cfg(feature = "raptor-10")]
mod rfc5053_codec;
#[cfg(feature = "raptor-10")]
mod rfc5053_layout;

#[cfg(feature = "raptor-q")]
pub use rfc6330_codec::{
    EncodingPacket as Rfc6330EncodingPacket, PayloadId as Rfc6330PayloadId, Rfc6330Decoder,
    Rfc6330Encoder, Rfc6330EncoderBuilder, Rfc6330TransmissionParams,
};
#[cfg(feature = "raptor-10")]
pub use rfc5053_codec::{
    EncodingPacket as Rfc5053EncodingPacket, PayloadId as Rfc5053PayloadId, Rfc5053Decoder,
    Rfc5053Encoder, Rfc5053EncoderBuilder, Rfc5053TransmissionParams,
};

#[cfg(feature = "raptor-q")]
use raptor_q::LDPCType;

pub(crate) const DEFAULT_DMAX: usize = 30;

#[cfg(feature = "raptor-q")]
pub(crate) fn default_raptor_q_ldpc_type() -> LDPCType {
    LDPCType::ReversedLDPC
}

use fountain_engine::traits::DataOperator;
use fountain_operators::{selected_kernel_kind, SimdDataOperator, SlabDataOperator};
#[cfg(feature = "raptor-10")]
use rfc5053_codec::EncodingPacket as Rfc5053Packet;
#[cfg(feature = "raptor-q")]
use rfc6330_codec::EncodingPacket as Rfc6330Packet;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind {
    RaptorQ,
    Raptor10,
}

#[derive(Debug, Clone)]
pub struct TransferPacket {
    pub sbn: u32,
    pub esi: usize,
    pub symbol: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum CodecConfig {
    #[cfg(feature = "raptor-q")]
    RaptorQ,
    #[cfg(feature = "raptor-10")]
    Raptor10,
}

#[derive(Debug, Clone)]
pub struct TransferSpec {
    pub transfer_length_f: usize,
    pub symbol_size_t: usize,
    pub codec: CodecConfig,
}

impl TransferSpec {
    /// Build a spec from object size, symbol size, and codec kind (codec internals are fixed in this crate).
    pub fn new(
        transfer_length_f: usize,
        symbol_size_t: usize,
        kind: CodecKind,
    ) -> Result<Self, TransferError> {
        Ok(Self {
            transfer_length_f,
            symbol_size_t,
            codec: codec_config_from_kind(kind)?,
        })
    }

    pub fn codec_kind(&self) -> CodecKind {
        match &self.codec {
            #[cfg(feature = "raptor-q")]
            CodecConfig::RaptorQ => CodecKind::RaptorQ,
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 => CodecKind::Raptor10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    InvalidObjectLength { expected: usize, actual: usize },
    InvalidSymbolSize,
    InvalidTransferLength,
    UnsupportedSourceBlock(u32),
    SymbolLengthMismatch { expected: usize, actual: usize },
    DecodeNotComplete,
    CodecNotEnabled(CodecKind),
}

impl Display for TransferError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidObjectLength { expected, actual } => {
                write!(f, "object length mismatch: expected {expected}, got {actual}")
            }
            Self::InvalidSymbolSize => write!(f, "symbol_size_t must be > 0"),
            Self::InvalidTransferLength => write!(f, "transfer_length_f must be > 0"),
            Self::UnsupportedSourceBlock(sbn) => {
                write!(f, "only sbn=0 is supported in v1, got {sbn}")
            }
            Self::SymbolLengthMismatch { expected, actual } => {
                write!(f, "symbol length mismatch: expected {expected}, got {actual}")
            }
            Self::DecodeNotComplete => write!(f, "decode is not complete yet"),
            Self::CodecNotEnabled(kind) => write!(
                f,
                "codec {kind:?} not enabled; rebuild fountain_transfer_core with feature {}",
                codec_feature_name(*kind)
            ),
        }
    }
}

impl Error for TransferError {}

fn codec_feature_name(kind: CodecKind) -> &'static str {
    match kind {
        CodecKind::RaptorQ => "raptor-q",
        CodecKind::Raptor10 => "raptor-10",
    }
}

pub trait TransferEncoder {
    fn source_packets(&mut self) -> Vec<TransferPacket>;
    fn repair_packets(
        &mut self,
        start_repair_index: usize,
        count: usize,
    ) -> Vec<TransferPacket>;
}

pub trait TransferDecoder {
    fn add_packet(&mut self, packet: &TransferPacket) -> Result<DecodeStatus, TransferError>;
    fn decode_status(&self) -> DecodeStatus;
    fn recover_object(&self) -> Result<Vec<u8>, TransferError>;
}

pub trait StorageManager {
    fn symbol_size(&self) -> usize;
    fn new_operator(&self) -> Box<dyn DataOperator>;
    fn store_packet(&mut self, packet: TransferPacket) -> usize;
    fn packet(&self, handle: usize) -> Option<&TransferPacket>;
}

pub struct SlabStorageManager {
    symbol_size_t: usize,
    prefer_simd: bool,
    packets: Vec<TransferPacket>,
}

impl SlabStorageManager {
    #[must_use]
    pub fn new(symbol_size_t: usize) -> Self {
        Self {
            symbol_size_t,
            prefer_simd: true,
            packets: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_kernel_preference(symbol_size_t: usize, prefer_simd: bool) -> Self {
        Self {
            symbol_size_t,
            prefer_simd,
            packets: Vec::new(),
        }
    }
}

impl StorageManager for SlabStorageManager {
    fn symbol_size(&self) -> usize {
        self.symbol_size_t
    }

    fn new_operator(&self) -> Box<dyn DataOperator> {
        if self.prefer_simd && selected_kernel_kind().is_simd() {
            Box::new(SimdDataOperator::new(self.symbol_size_t))
        } else {
            Box::new(SlabDataOperator::new(self.symbol_size_t))
        }
    }

    fn store_packet(&mut self, packet: TransferPacket) -> usize {
        self.packets.push(packet);
        self.packets.len() - 1
    }

    fn packet(&self, handle: usize) -> Option<&TransferPacket> {
        self.packets.get(handle)
    }
}

pub enum TransferEncoderImpl {
    #[cfg(feature = "raptor-q")]
    RaptorQ(Rfc6330Encoder, TransferSpec),
    #[cfg(feature = "raptor-10")]
    Raptor10(Rfc5053Encoder, TransferSpec),
}

impl TransferEncoderImpl {
    pub fn new(spec: TransferSpec, object: &[u8]) -> Result<Self, TransferError> {
        validate_spec(&spec)?;
        if object.len() != spec.transfer_length_f {
            return Err(TransferError::InvalidObjectLength {
                expected: spec.transfer_length_f,
                actual: object.len(),
            });
        }
        let encoder = match &spec.codec {
            #[cfg(feature = "raptor-q")]
            CodecConfig::RaptorQ => {
                let params = Rfc6330TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::RaptorQ(
                    Rfc6330Encoder::new_with_default_operator(params, object),
                    spec.clone(),
                )
            }
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 => {
                let params = Rfc5053TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::Raptor10(
                    Rfc5053Encoder::new_with_default_operator(params, object),
                    spec.clone(),
                )
            }
        };
        Ok(encoder)
    }

    pub fn new_with_storage_manager(
        spec: TransferSpec,
        object: &[u8],
        manager: &dyn StorageManager,
    ) -> Result<Self, TransferError> {
        validate_spec(&spec)?;
        if manager.symbol_size() != spec.symbol_size_t {
            return Err(TransferError::SymbolLengthMismatch {
                expected: spec.symbol_size_t,
                actual: manager.symbol_size(),
            });
        }
        if object.len() != spec.transfer_length_f {
            return Err(TransferError::InvalidObjectLength {
                expected: spec.transfer_length_f,
                actual: object.len(),
            });
        }
        let encoder = match &spec.codec {
            #[cfg(feature = "raptor-q")]
            CodecConfig::RaptorQ => {
                let params = Rfc6330TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::RaptorQ(
                    Rfc6330Encoder::new_with_operator(params, object, manager.new_operator()),
                    spec.clone(),
                )
            }
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 => {
                let params = Rfc5053TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::Raptor10(
                    Rfc5053Encoder::new_with_operator(params, object, manager.new_operator()),
                    spec.clone(),
                )
            }
        };
        Ok(encoder)
    }
}

impl TransferEncoder for TransferEncoderImpl {
    fn source_packets(&mut self) -> Vec<TransferPacket> {
        match self {
            #[cfg(feature = "raptor-q")]
            Self::RaptorQ(enc, _) => enc
                .source_packets()
                .into_iter()
                .map(|pkt| TransferPacket {
                    sbn: pkt.payload_id.sbn,
                    esi: pkt.payload_id.esi,
                    symbol: pkt.symbol,
                })
                .collect(),
            #[cfg(feature = "raptor-10")]
            Self::Raptor10(enc, _) => enc
                .source_packets()
                .into_iter()
                .map(|pkt| TransferPacket {
                    sbn: pkt.payload_id.sbn,
                    esi: pkt.payload_id.esi,
                    symbol: pkt.symbol,
                })
                .collect(),
        }
    }

    fn repair_packets(
        &mut self,
        start_repair_index: usize,
        count: usize,
    ) -> Vec<TransferPacket> {
        match self {
            #[cfg(feature = "raptor-q")]
            Self::RaptorQ(enc, _) => enc
                .repair_packets(start_repair_index, count)
                .into_iter()
                .map(|pkt| TransferPacket {
                    sbn: pkt.payload_id.sbn,
                    esi: pkt.payload_id.esi,
                    symbol: pkt.symbol,
                })
                .collect(),
            #[cfg(feature = "raptor-10")]
            Self::Raptor10(enc, _) => enc
                .repair_packets(start_repair_index, count)
                .into_iter()
                .map(|pkt| TransferPacket {
                    sbn: pkt.payload_id.sbn,
                    esi: pkt.payload_id.esi,
                    symbol: pkt.symbol,
                })
                .collect(),
        }
    }
}

pub enum TransferDecoderImpl {
    #[cfg(feature = "raptor-q")]
    RaptorQ(Rfc6330Decoder, TransferSpec),
    #[cfg(feature = "raptor-10")]
    Raptor10(Rfc5053Decoder, TransferSpec),
}

impl TransferDecoderImpl {
    pub fn new(spec: TransferSpec) -> Result<Self, TransferError> {
        validate_spec(&spec)?;
        let decoder = match &spec.codec {
            #[cfg(feature = "raptor-q")]
            CodecConfig::RaptorQ => {
                let params = Rfc6330TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::RaptorQ(Rfc6330Decoder::new_with_default_operator(params), spec)
            }
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 => {
                let params = Rfc5053TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::Raptor10(Rfc5053Decoder::new_with_default_operator(params), spec)
            }
        };
        Ok(decoder)
    }

    pub fn new_with_storage_manager(
        spec: TransferSpec,
        manager: &dyn StorageManager,
    ) -> Result<Self, TransferError> {
        validate_spec(&spec)?;
        if manager.symbol_size() != spec.symbol_size_t {
            return Err(TransferError::SymbolLengthMismatch {
                expected: spec.symbol_size_t,
                actual: manager.symbol_size(),
            });
        }
        let decoder = match &spec.codec {
            #[cfg(feature = "raptor-q")]
            CodecConfig::RaptorQ => {
                let params = Rfc6330TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::RaptorQ(
                    Rfc6330Decoder::new_with_operator(params, manager.new_operator()),
                    spec,
                )
            }
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 => {
                let params = Rfc5053TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::Raptor10(
                    Rfc5053Decoder::new_with_operator(params, manager.new_operator()),
                    spec,
                )
            }
        };
        Ok(decoder)
    }
}

impl TransferDecoder for TransferDecoderImpl {
    fn add_packet(&mut self, packet: &TransferPacket) -> Result<DecodeStatus, TransferError> {
        if packet.sbn != 0 {
            return Err(TransferError::UnsupportedSourceBlock(packet.sbn));
        }
        let symbol_size = match self {
            #[cfg(feature = "raptor-q")]
            Self::RaptorQ(_, spec) => spec.symbol_size_t,
            #[cfg(feature = "raptor-10")]
            Self::Raptor10(_, spec) => spec.symbol_size_t,
        };
        if packet.symbol.len() != symbol_size {
            return Err(TransferError::SymbolLengthMismatch {
                expected: symbol_size,
                actual: packet.symbol.len(),
            });
        }
        let status = match self {
            #[cfg(feature = "raptor-q")]
            Self::RaptorQ(dec, _) => dec.add_packet(&Rfc6330Packet {
                payload_id: Rfc6330PayloadId {
                    sbn: packet.sbn,
                    esi: packet.esi,
                },
                symbol: packet.symbol.clone(),
            }),
            #[cfg(feature = "raptor-10")]
            Self::Raptor10(dec, _) => dec.add_packet(&Rfc5053Packet {
                payload_id: Rfc5053PayloadId {
                    sbn: packet.sbn,
                    esi: packet.esi,
                },
                symbol: packet.symbol.clone(),
            }),
        };
        Ok(status)
    }

    fn decode_status(&self) -> DecodeStatus {
        match self {
            #[cfg(feature = "raptor-q")]
            Self::RaptorQ(dec, _) => dec.decode_status(),
            #[cfg(feature = "raptor-10")]
            Self::Raptor10(dec, _) => dec.decode_status(),
        }
    }

    fn recover_object(&self) -> Result<Vec<u8>, TransferError> {
        if self.decode_status() != DecodeStatus::Decoded {
            return Err(TransferError::DecodeNotComplete);
        }
        let object = match self {
            #[cfg(feature = "raptor-q")]
            Self::RaptorQ(dec, _) => dec.recover_object(),
            #[cfg(feature = "raptor-10")]
            Self::Raptor10(dec, _) => dec.recover_object(),
        };
        Ok(object)
    }
}

fn validate_spec(spec: &TransferSpec) -> Result<(), TransferError> {
    if spec.transfer_length_f == 0 {
        return Err(TransferError::InvalidTransferLength);
    }
    if spec.symbol_size_t == 0 {
        return Err(TransferError::InvalidSymbolSize);
    }
    ensure_codec_enabled(spec.codec_kind())
}

/// Build [`CodecConfig`] for a [`CodecKind`] when the corresponding crate feature is enabled.
pub fn codec_config_from_kind(kind: CodecKind) -> Result<CodecConfig, TransferError> {
    ensure_codec_enabled(kind)?;
    match kind {
        CodecKind::RaptorQ => {
            #[cfg(feature = "raptor-q")]
            {
                Ok(CodecConfig::RaptorQ)
            }
            #[cfg(not(feature = "raptor-q"))]
            {
                Err(TransferError::CodecNotEnabled(kind))
            }
        }
        CodecKind::Raptor10 => {
            #[cfg(feature = "raptor-10")]
            {
                Ok(CodecConfig::Raptor10)
            }
            #[cfg(not(feature = "raptor-10"))]
            {
                Err(TransferError::CodecNotEnabled(kind))
            }
        }
    }
}

fn ensure_codec_enabled(kind: CodecKind) -> Result<(), TransferError> {
    match kind {
        CodecKind::RaptorQ => {
            #[cfg(not(feature = "raptor-q"))]
            return Err(TransferError::CodecNotEnabled(kind));
            #[cfg(feature = "raptor-q")]
            Ok(())
        }
        CodecKind::Raptor10 => {
            #[cfg(not(feature = "raptor-10"))]
            return Err(TransferError::CodecNotEnabled(kind));
            #[cfg(feature = "raptor-10")]
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_object(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    #[cfg(all(feature = "raptor-q", feature = "raptor-10"))]
    fn roundtrip_dual_codec_with_storage_manager() {
        let specs = vec![
            TransferSpec::new(199, 10, CodecKind::RaptorQ).expect("raptor-q spec"),
            TransferSpec::new(77, 8, CodecKind::Raptor10).expect("raptor-10 spec"),
        ];

        for spec in specs {
            let object = sample_object(spec.transfer_length_f);
            let mut storage = SlabStorageManager::new(spec.symbol_size_t);
            let mut enc = TransferEncoderImpl::new_with_storage_manager(
                spec.clone(),
                &object,
                &storage,
            )
            .expect("encoder");
            let mut dec =
                TransferDecoderImpl::new_with_storage_manager(spec.clone(), &storage).expect("decoder");

            let source = enc.source_packets();
            for packet in source.iter().skip(1) {
                let handle = storage.store_packet(packet.clone());
                let status = dec
                    .add_packet(storage.packet(handle).expect("packet in manager"))
                    .expect("add source packet");
                if status == DecodeStatus::Decoded {
                    break;
                }
            }
            let repairs = enc.repair_packets(0, 64);
            for repair in repairs {
                let handle = storage.store_packet(repair);
                let status = dec
                    .add_packet(storage.packet(handle).expect("packet in manager"))
                    .expect("add repair packet");
                if status == DecodeStatus::Decoded {
                    break;
                }
            }

            assert_eq!(dec.decode_status(), DecodeStatus::Decoded);
            assert_eq!(dec.recover_object().expect("recover"), object);
        }
    }

    #[test]
    #[cfg(feature = "raptor-10")]
    fn rejects_malformed_packet() {
        let spec = TransferSpec::new(77, 8, CodecKind::Raptor10).expect("spec");
        let mut dec = TransferDecoderImpl::new(spec).expect("decoder");

        let err = dec
            .add_packet(&TransferPacket {
                sbn: 1,
                esi: 0,
                symbol: vec![0u8; 8],
            })
            .expect_err("must reject sbn != 0");
        assert_eq!(err, TransferError::UnsupportedSourceBlock(1));

        let err = dec
            .add_packet(&TransferPacket {
                sbn: 0,
                esi: 0,
                symbol: vec![0u8; 7],
            })
            .expect_err("must reject symbol size mismatch");
        assert_eq!(
            err,
            TransferError::SymbolLengthMismatch {
                expected: 8,
                actual: 7
            }
        );
    }

    #[test]
    #[cfg(feature = "raptor-q")]
    fn recover_requires_decoded_status() {
        let spec = TransferSpec::new(199, 10, CodecKind::RaptorQ).expect("spec");
        let dec = TransferDecoderImpl::new(spec).expect("decoder");
        assert_eq!(
            dec.recover_object().expect_err("not decoded yet"),
            TransferError::DecodeNotComplete
        );
    }

    #[test]
    #[cfg(all(feature = "raptor-q", feature = "raptor-10"))]
    fn packet_loss_matrix_recovers_for_both_codecs() {
        let scenarios = vec![
            TransferSpec::new(199, 10, CodecKind::RaptorQ).expect("raptor-q spec"),
            TransferSpec::new(77, 8, CodecKind::Raptor10).expect("raptor-10 spec"),
        ];
        let loss_steps = [2usize, 3usize, 5usize];

        for spec in scenarios {
            let object = sample_object(spec.transfer_length_f);
            for step in loss_steps {
                let mut enc = TransferEncoderImpl::new(spec.clone(), &object).expect("encoder");
                let mut dec = TransferDecoderImpl::new(spec.clone()).expect("decoder");

                for (idx, packet) in enc.source_packets().into_iter().enumerate() {
                    if idx % step != 0 {
                        let _ = dec.add_packet(&packet).expect("add source");
                    }
                }

                let repairs = enc.repair_packets(0, 128);
                for repair in repairs {
                    if dec.add_packet(&repair).expect("add repair") == DecodeStatus::Decoded {
                        break;
                    }
                }

                assert_eq!(
                    dec.decode_status(),
                    DecodeStatus::Decoded,
                    "failed with loss step {step} for {:?}",
                    spec.codec_kind()
                );
                assert_eq!(dec.recover_object().expect("recover"), object);
            }
        }
    }

    #[test]
    #[cfg(feature = "raptor-q")]
    fn raptor_q_roundtrip_default_operator() {
        let object = sample_object(199);
        let spec = TransferSpec::new(object.len(), 10, CodecKind::RaptorQ).expect("spec");
        let mut enc = TransferEncoderImpl::new(spec.clone(), &object).expect("encoder");
        let mut dec = TransferDecoderImpl::new(spec).expect("decoder");
        for packet in enc.source_packets() {
            let _ = dec.add_packet(&packet).expect("source");
        }
        for packet in enc.repair_packets(0, 64) {
            if dec.add_packet(&packet).expect("repair") == DecodeStatus::Decoded {
                break;
            }
        }
        assert_eq!(dec.recover_object().expect("recover"), object);
    }

    #[test]
    #[cfg(feature = "raptor-10")]
    fn raptor_10_roundtrip_default_operator() {
        let object = sample_object(77);
        let spec = TransferSpec::new(object.len(), 8, CodecKind::Raptor10).expect("spec");
        let mut enc = TransferEncoderImpl::new(spec.clone(), &object).expect("encoder");
        let mut dec = TransferDecoderImpl::new(spec).expect("decoder");
        for packet in enc.source_packets() {
            let _ = dec.add_packet(&packet).expect("source");
        }
        for packet in enc.repair_packets(0, 64) {
            if dec.add_packet(&packet).expect("repair") == DecodeStatus::Decoded {
                break;
            }
        }
        assert_eq!(dec.recover_object().expect("recover"), object);
    }
}
