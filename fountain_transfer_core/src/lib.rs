//! Transfer facade over RaptorQ, Raptor10, and optional BHP.
//!
//! See the [crate README](../README.md) for usage, API tables, and v1 limitations.
//!
//! # Features
//!
//! - **`raptor-q`** (default) — RFC 6330 via `fountain_scheme::rfc::rq`; enables [`CodecConfig::RaptorQ`].
//! - **`raptor-10`** (default) — RFC 5053 via `fountain_scheme::rfc::r10`; enables [`CodecConfig::Raptor10`].
//! - **`bhp`** (optional) — BinaryHdpcPi via `fountain_scheme::BinaryHdpcPi`; enables [`CodecConfig::Bhp`].
//!
//! Build with only one codec, e.g. `cargo build -p fountain_transfer_core --no-default-features --features raptor-q`.
//! Enable BHP with `--features bhp` (not in the default feature set). With no codec
//! features enabled, the crate still builds for API/documentation tooling, but all
//! codec constructors return [`TransferError::CodecNotEnabled`].
//!
//! # Main types
//!
//! - [`TransferSpec`] / [`CodecConfig`] — object length, symbol size, codec choice
//! - [`TransferPacket`] — RFC-style `{ sbn, esi, symbol }`
//! - [`TransferEncoder`] / [`TransferDecoder`] — traits implemented by [`TransferEncoderImpl`] / [`TransferDecoderImpl`]
//! - [`StorageManager`] / [`SlabStorageManager`] — optional `fountain_operators` integration
//!
//! [`DEFAULT_DMAX`] and a reversed RaptorQ LDPC precode are used by
//! [`codec_config_from_kind`]; custom values can be supplied through [`CodecConfig`].
//!
//! Re-exported for integrators: [`DecodeStatus`].

#![cfg_attr(
    not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")),
    allow(unreachable_code, unused_variables, unused_mut)
)]

pub use fountain_engine::types::DecodeStatus;

#[cfg(feature = "raptor-10")]
mod rfc5053_codec;
#[cfg(any(feature = "raptor-10", feature = "bhp"))]
mod rfc5053_layout;
#[cfg(feature = "bhp")]
mod bhp_codec;
#[cfg(feature = "raptor-q")]
mod rfc6330_codec;
#[cfg(feature = "raptor-q")]
pub mod rfc6330_layout;
#[cfg(feature = "raptor-q")]
mod rfc6330_mapping;

#[cfg(feature = "raptor-10")]
pub use rfc5053_codec::{
    EncodingPacket as Rfc5053EncodingPacket, PayloadId as Rfc5053PayloadId, Rfc5053Decoder,
    Rfc5053Encoder, Rfc5053EncoderBuilder, Rfc5053TransmissionParams,
};
#[cfg(feature = "bhp")]
pub use bhp_codec::{
    EncodingPacket as BhpEncodingPacket, PayloadId as BhpPayloadId, BhpDecoder, BhpEncoder,
    BhpEncoderBuilder, BhpTransmissionParams,
};
#[cfg(feature = "raptor-q")]
pub use rfc6330_codec::{
    EncodingPacket as Rfc6330EncodingPacket, PayloadId as Rfc6330PayloadId, Rfc6330Decoder,
    Rfc6330Encoder, Rfc6330EncoderBuilder, Rfc6330TransmissionParams,
};

#[cfg(feature = "raptor-q")]
pub use fountain_scheme::rfc::rq::LDPCType;

use fountain_engine::traits::DataOperator;
use fountain_operators::SlabDataOperator;
#[cfg(feature = "raptor-10")]
use rfc5053_codec::EncodingPacket as Rfc5053Packet;
#[cfg(feature = "bhp")]
use bhp_codec::EncodingPacket as BhpPacket;
#[cfg(feature = "raptor-q")]
use rfc6330_codec::EncodingPacket as Rfc6330Packet;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Default maximum degree (`dmax`) for RaptorQ and Raptor10 in this facade.
pub const DEFAULT_DMAX: usize = 30;

/// Codec family used for a transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind {
    /// RaptorQ, as specified by RFC 6330.
    RaptorQ,
    /// Raptor10, as specified by RFC 5053.
    Raptor10,
    /// BHP (BinaryHdpcPi) — binary RQ-triangle HDPC + message PI.
    Bhp,
}

impl Display for CodecKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RaptorQ => "RaptorQ",
            Self::Raptor10 => "Raptor10",
            Self::Bhp => "BHP",
        })
    }
}

/// Codec-neutral RFC payload sent over a transport.
///
/// `sbn` and `esi` are payload identifiers, not the codec engine's internal coded IDs.
#[derive(Debug, Clone)]
pub struct TransferPacket {
    /// Source block number. The transfer facade currently supports only block `0`.
    pub sbn: u32,
    /// Encoding symbol identifier within the source block.
    pub esi: usize,
    /// Encoded symbol bytes.
    pub symbol: Vec<u8>,
}

/// Codec-specific configuration for a transfer.
#[derive(Debug, Clone)]
pub enum CodecConfig {
    /// RFC 6330 configuration.
    #[cfg(feature = "raptor-q")]
    RaptorQ {
        /// Maximum degree used by the scheme configuration.
        dmax: usize,
        /// LDPC precode type.
        ldpc_type: LDPCType,
    },
    /// RFC 5053 configuration.
    #[cfg(feature = "raptor-10")]
    Raptor10 {
        /// Maximum degree used by the scheme configuration.
        dmax: usize,
    },
    /// BHP (BinaryHdpcPi) configuration.
    #[cfg(feature = "bhp")]
    Bhp,
    /// Placeholder used when no codec feature is enabled.
    ///
    /// `Infallible` makes this variant impossible to construct.
    #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
    #[doc(hidden)]
    NoCodec(std::convert::Infallible),
}

/// Description of the object and codec needed by both encoder and decoder.
#[derive(Debug, Clone)]
pub struct TransferSpec {
    /// Original, unpadded object length in bytes.
    pub transfer_length_f: usize,
    /// Number of bytes in each encoded symbol.
    pub symbol_size_t: usize,
    /// Codec and codec-specific settings.
    pub codec: CodecConfig,
}

impl TransferSpec {
    /// Build a validated spec using [`DEFAULT_DMAX`] and the default RaptorQ LDPC.
    ///
    /// Returns [`TransferError::InvalidTransferLength`] or
    /// [`TransferError::InvalidSymbolSize`] for zero-sized dimensions, and
    /// [`TransferError::CodecNotEnabled`] when the selected feature is disabled.
    pub fn new(
        transfer_length_f: usize,
        symbol_size_t: usize,
        kind: CodecKind,
    ) -> Result<Self, TransferError> {
        validate_dimensions(transfer_length_f, symbol_size_t)?;
        Self::with_codec(
            transfer_length_f,
            symbol_size_t,
            codec_config_from_kind(kind)?,
        )
    }

    /// Build a validated spec with custom codec settings.
    ///
    /// This is the preferred constructor when overriding `dmax` or RaptorQ's
    /// LDPC precode. [`TransferSpec::validate`] performs the same checks for a
    /// struct literal.
    pub fn with_codec(
        transfer_length_f: usize,
        symbol_size_t: usize,
        codec: CodecConfig,
    ) -> Result<Self, TransferError> {
        let spec = Self {
            transfer_length_f,
            symbol_size_t,
            codec,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Validate dimensions and confirm that the configured codec is enabled.
    pub fn validate(&self) -> Result<(), TransferError> {
        validate_spec(self)
    }

    /// Return the codec family selected by this spec.
    pub fn codec_kind(&self) -> CodecKind {
        match &self.codec {
            #[cfg(feature = "raptor-q")]
            CodecConfig::RaptorQ { .. } => CodecKind::RaptorQ,
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 { .. } => CodecKind::Raptor10,
            #[cfg(feature = "bhp")]
            CodecConfig::Bhp => CodecKind::Bhp,
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            CodecConfig::NoCodec(never) => match *never {},
        }
    }
}

/// Errors returned while validating transfer parameters or decoding packets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    /// The object bytes do not match the length declared in the spec.
    InvalidObjectLength { expected: usize, actual: usize },
    /// The configured symbol size is zero.
    InvalidSymbolSize,
    /// The configured object length is zero.
    InvalidTransferLength,
    /// A packet used a source block other than the currently supported block zero.
    UnsupportedSourceBlock(u32),
    /// A packet's symbol length does not match the negotiated symbol size.
    SymbolLengthMismatch { expected: usize, actual: usize },
    /// Recovery was requested before decoding reached [`DecodeStatus::Decoded`].
    DecodeNotComplete,
    /// The requested codec was not compiled into the crate.
    CodecNotEnabled(CodecKind),
}

impl Display for TransferError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidObjectLength { expected, actual } => {
                write!(
                    f,
                    "input object is {actual} bytes but the transfer spec expects {expected} bytes"
                )
            }
            Self::InvalidSymbolSize => {
                write!(f, "symbol size must be greater than zero")
            }
            Self::InvalidTransferLength => {
                write!(f, "input object must not be empty")
            }
            Self::UnsupportedSourceBlock(sbn) => {
                write!(
                    f,
                    "received packet for source block {sbn}, but only source block 0 is supported"
                )
            }
            Self::SymbolLengthMismatch { expected, actual } => {
                write!(
                    f,
                    "received a {actual}-byte symbol, but the transfer expects {expected} bytes"
                )
            }
            Self::DecodeNotComplete => {
                write!(
                    f,
                    "cannot recover the object until decoding completes; add more packets"
                )
            }
            Self::CodecNotEnabled(kind) => write!(
                f,
                "{kind} codec support is disabled; rebuild with the `{}` feature",
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
        CodecKind::Bhp => "bhp",
    }
}

/// Produces source and repair packets from an object.
pub trait TransferEncoder {
    /// Return all source packets for the object.
    fn source_packets(&mut self) -> Vec<TransferPacket>;
    /// Return `count` repair packets beginning at `start_repair_index`.
    fn repair_packets(&mut self, start_repair_index: usize, count: usize) -> Vec<TransferPacket>;
}

/// Consumes packets and recovers the original object.
pub trait TransferDecoder {
    /// Add one received symbol and return the resulting decode status.
    ///
    /// Systematic ESIs (`esi < K`) use the engine single-copy message-slot path.
    /// When the attached operator already holds the payload at `data_id == esi`, call
    /// [`Self::mark_systematic_resident`] (or [`Self::mark_all_systematic_resident`])
    /// for the zero-copy / all-K fast path instead of [`Self::add_packet`].
    fn add_packet(&mut self, packet: &TransferPacket) -> Result<DecodeStatus, TransferError>;
    /// Mark systematic message slot `esi` already resident in the decoder operator.
    ///
    /// No payload copy. See engine [`Decoder::mark_systematic_resident`].
    fn mark_systematic_resident(&mut self, esi: usize) -> Result<DecodeStatus, TransferError>;
    /// Mark every systematic slot `0..K` resident (zero-copy all-K when payloads are in place).
    fn mark_all_systematic_resident(&mut self) -> Result<DecodeStatus, TransferError>;
    /// Return the current decode state without changing the decoder.
    fn decode_status(&self) -> DecodeStatus;
    /// Recover the original object once decoding is complete.
    fn recover_object(&self) -> Result<Vec<u8>, TransferError>;
}

/// Supplies codec operators and optionally stores packets for a decoder.
pub trait StorageManager {
    /// Symbol size expected by the supplied operators.
    fn symbol_size(&self) -> usize;
    /// Create a fresh data operator for an encoder or decoder.
    fn new_operator(&self) -> Box<dyn DataOperator>;
    /// Store a packet and return a handle for [`StorageManager::packet`].
    fn store_packet(&mut self, packet: TransferPacket) -> usize;
    /// Retrieve a previously stored packet.
    fn packet(&self, handle: usize) -> Option<&TransferPacket>;
}

/// In-memory [`StorageManager`] backed by a growable vector.
pub struct SlabStorageManager {
    symbol_size_t: usize,
    packets: Vec<TransferPacket>,
}

impl SlabStorageManager {
    /// Create an empty packet store for symbols of `symbol_size_t` bytes.
    #[must_use]
    pub fn new(symbol_size_t: usize) -> Self {
        Self {
            symbol_size_t,
            packets: Vec::new(),
        }
    }
}

impl StorageManager for SlabStorageManager {
    fn symbol_size(&self) -> usize {
        self.symbol_size_t
    }

    fn new_operator(&self) -> Box<dyn DataOperator> {
        Box::new(SlabDataOperator::new(self.symbol_size_t))
    }

    fn store_packet(&mut self, packet: TransferPacket) -> usize {
        self.packets.push(packet);
        self.packets.len() - 1
    }

    fn packet(&self, handle: usize) -> Option<&TransferPacket> {
        self.packets.get(handle)
    }
}

/// Codec-dispatched encoder returned by [`TransferEncoderImpl::new`].
pub enum TransferEncoderImpl {
    #[cfg(feature = "raptor-q")]
    RaptorQ(Rfc6330Encoder, TransferSpec),
    #[cfg(feature = "raptor-10")]
    Raptor10(Rfc5053Encoder, TransferSpec),
    #[cfg(feature = "bhp")]
    Bhp(BhpEncoder, TransferSpec),
    #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
    #[doc(hidden)]
    NoCodec(std::convert::Infallible),
}

impl TransferEncoderImpl {
    /// Create an encoder from a validated spec and matching object bytes.
    ///
    /// Use [`TransferSpec::validate`] or [`TransferSpec::new`] at configuration
    /// boundaries so parameter errors are reported before encoding starts.
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
            CodecConfig::RaptorQ { dmax, ldpc_type } => {
                let params = Rfc6330TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                    *dmax,
                    ldpc_type.clone(),
                );
                Self::RaptorQ(
                    Rfc6330Encoder::new_with_default_operator(params, object),
                    spec.clone(),
                )
            }
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 { dmax } => {
                let params = Rfc5053TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                    *dmax,
                );
                Self::Raptor10(
                    Rfc5053Encoder::new_with_default_operator(params, object),
                    spec.clone(),
                )
            }
            #[cfg(feature = "bhp")]
            CodecConfig::Bhp => {
                let params = BhpTransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::Bhp(
                    BhpEncoder::new_with_default_operator(params, object),
                    spec.clone(),
                )
            }
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            CodecConfig::NoCodec(never) => match *never {},
        };
        Ok(encoder)
    }

    /// Create an encoder using operators supplied by `manager`.
    ///
    /// The manager's symbol size must match the spec.
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
            CodecConfig::RaptorQ { dmax, ldpc_type } => {
                let params = Rfc6330TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                    *dmax,
                    ldpc_type.clone(),
                );
                Self::RaptorQ(
                    Rfc6330Encoder::new_with_operator(params, object, manager.new_operator()),
                    spec.clone(),
                )
            }
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 { dmax } => {
                let params = Rfc5053TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                    *dmax,
                );
                Self::Raptor10(
                    Rfc5053Encoder::new_with_operator(params, object, manager.new_operator()),
                    spec.clone(),
                )
            }
            #[cfg(feature = "bhp")]
            CodecConfig::Bhp => {
                let params = BhpTransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::Bhp(
                    BhpEncoder::new_with_operator(params, object, manager.new_operator()),
                    spec.clone(),
                )
            }
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            CodecConfig::NoCodec(never) => match *never {},
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
            #[cfg(feature = "bhp")]
            Self::Bhp(enc, _) => enc
                .source_packets()
                .into_iter()
                .map(|pkt| TransferPacket {
                    sbn: pkt.payload_id.sbn,
                    esi: pkt.payload_id.esi,
                    symbol: pkt.symbol,
                })
                .collect(),
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            Self::NoCodec(never) => match *never {},
        }
    }

    fn repair_packets(&mut self, start_repair_index: usize, count: usize) -> Vec<TransferPacket> {
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
            #[cfg(feature = "bhp")]
            Self::Bhp(enc, _) => enc
                .repair_packets(start_repair_index, count)
                .into_iter()
                .map(|pkt| TransferPacket {
                    sbn: pkt.payload_id.sbn,
                    esi: pkt.payload_id.esi,
                    symbol: pkt.symbol,
                })
                .collect(),
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            Self::NoCodec(never) => match *never {},
        }
    }
}

/// Codec-dispatched decoder returned by [`TransferDecoderImpl::new`].
pub enum TransferDecoderImpl {
    #[cfg(feature = "raptor-q")]
    RaptorQ(Rfc6330Decoder, TransferSpec),
    #[cfg(feature = "raptor-10")]
    Raptor10(Rfc5053Decoder, TransferSpec),
    #[cfg(feature = "bhp")]
    Bhp(BhpDecoder, TransferSpec),
    #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
    #[doc(hidden)]
    NoCodec(std::convert::Infallible),
}

impl TransferDecoderImpl {
    /// Create a decoder from a validated spec.
    pub fn new(spec: TransferSpec) -> Result<Self, TransferError> {
        validate_spec(&spec)?;
        let decoder = match &spec.codec {
            #[cfg(feature = "raptor-q")]
            CodecConfig::RaptorQ { dmax, ldpc_type } => {
                let params = Rfc6330TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                    *dmax,
                    ldpc_type.clone(),
                );
                Self::RaptorQ(Rfc6330Decoder::new_with_default_operator(params), spec)
            }
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 { dmax } => {
                let params = Rfc5053TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                    *dmax,
                );
                Self::Raptor10(Rfc5053Decoder::new_with_default_operator(params), spec)
            }
            #[cfg(feature = "bhp")]
            CodecConfig::Bhp => {
                let params = BhpTransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::Bhp(BhpDecoder::new_with_default_operator(params), spec)
            }
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            CodecConfig::NoCodec(never) => match *never {},
        };
        Ok(decoder)
    }

    /// Create a decoder using an operator supplied by `manager`.
    ///
    /// The manager's symbol size must match the spec.
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
            CodecConfig::RaptorQ { dmax, ldpc_type } => {
                let params = Rfc6330TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                    *dmax,
                    ldpc_type.clone(),
                );
                Self::RaptorQ(
                    Rfc6330Decoder::new_with_operator(params, manager.new_operator()),
                    spec,
                )
            }
            #[cfg(feature = "raptor-10")]
            CodecConfig::Raptor10 { dmax } => {
                let params = Rfc5053TransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                    *dmax,
                );
                Self::Raptor10(
                    Rfc5053Decoder::new_with_operator(params, manager.new_operator()),
                    spec,
                )
            }
            #[cfg(feature = "bhp")]
            CodecConfig::Bhp => {
                let params = BhpTransmissionParams::for_object(
                    spec.transfer_length_f,
                    spec.symbol_size_t,
                );
                Self::Bhp(
                    BhpDecoder::new_with_operator(params, manager.new_operator()),
                    spec,
                )
            }
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            CodecConfig::NoCodec(never) => match *never {},
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
            #[cfg(feature = "bhp")]
            Self::Bhp(_, spec) => spec.symbol_size_t,
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            Self::NoCodec(never) => match *never {},
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
            #[cfg(feature = "bhp")]
            Self::Bhp(dec, _) => dec.add_packet(&BhpPacket {
                payload_id: BhpPayloadId {
                    sbn: packet.sbn,
                    esi: packet.esi,
                },
                symbol: packet.symbol.clone(),
            }),
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            Self::NoCodec(never) => match *never {},
        };
        Ok(status)
    }

    fn mark_systematic_resident(&mut self, esi: usize) -> Result<DecodeStatus, TransferError> {
        let status = match self {
            #[cfg(feature = "raptor-q")]
            Self::RaptorQ(dec, _) => dec.mark_systematic_resident(esi),
            #[cfg(feature = "raptor-10")]
            Self::Raptor10(dec, _) => dec.mark_systematic_resident(esi),
            #[cfg(feature = "bhp")]
            Self::Bhp(dec, _) => dec.mark_systematic_resident(esi),
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            Self::NoCodec(never) => match *never {},
        };
        Ok(status)
    }

    fn mark_all_systematic_resident(&mut self) -> Result<DecodeStatus, TransferError> {
        let status = match self {
            #[cfg(feature = "raptor-q")]
            Self::RaptorQ(dec, _) => dec.mark_all_systematic_resident(),
            #[cfg(feature = "raptor-10")]
            Self::Raptor10(dec, _) => dec.mark_all_systematic_resident(),
            #[cfg(feature = "bhp")]
            Self::Bhp(dec, _) => dec.mark_all_systematic_resident(),
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            Self::NoCodec(never) => match *never {},
        };
        Ok(status)
    }

    fn decode_status(&self) -> DecodeStatus {
        match self {
            #[cfg(feature = "raptor-q")]
            Self::RaptorQ(dec, _) => dec.decode_status(),
            #[cfg(feature = "raptor-10")]
            Self::Raptor10(dec, _) => dec.decode_status(),
            #[cfg(feature = "bhp")]
            Self::Bhp(dec, _) => dec.decode_status(),
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            Self::NoCodec(never) => match *never {},
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
            #[cfg(feature = "bhp")]
            Self::Bhp(dec, _) => dec.recover_object(),
            #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
            Self::NoCodec(never) => match *never {},
        };
        Ok(object)
    }
}

fn validate_spec(spec: &TransferSpec) -> Result<(), TransferError> {
    validate_dimensions(spec.transfer_length_f, spec.symbol_size_t)?;
    ensure_codec_enabled(spec.codec_kind())
}

fn validate_dimensions(
    transfer_length_f: usize,
    symbol_size_t: usize,
) -> Result<(), TransferError> {
    if transfer_length_f == 0 {
        return Err(TransferError::InvalidTransferLength);
    }
    if symbol_size_t == 0 {
        return Err(TransferError::InvalidSymbolSize);
    }
    Ok(())
}

/// Build [`CodecConfig`] for a [`CodecKind`] when the corresponding crate feature is enabled.
///
/// Uses [`DEFAULT_DMAX`] and the crate's default RaptorQ LDPC precode (reversed LDPC).
pub fn codec_config_from_kind(kind: CodecKind) -> Result<CodecConfig, TransferError> {
    build_codec_config(kind, DEFAULT_DMAX)
}

fn build_codec_config(kind: CodecKind, dmax: usize) -> Result<CodecConfig, TransferError> {
    ensure_codec_enabled(kind)?;
    match kind {
        CodecKind::RaptorQ => {
            #[cfg(feature = "raptor-q")]
            {
                Ok(CodecConfig::RaptorQ {
                    dmax,
                    ldpc_type: raptor_q_ldpc_type(),
                })
            }
            #[cfg(not(feature = "raptor-q"))]
            {
                Err(TransferError::CodecNotEnabled(kind))
            }
        }
        CodecKind::Raptor10 => {
            #[cfg(feature = "raptor-10")]
            {
                Ok(CodecConfig::Raptor10 { dmax })
            }
            #[cfg(not(feature = "raptor-10"))]
            {
                Err(TransferError::CodecNotEnabled(kind))
            }
        }
        CodecKind::Bhp => {
            #[cfg(feature = "bhp")]
            {
                let _ = dmax;
                Ok(CodecConfig::Bhp)
            }
            #[cfg(not(feature = "bhp"))]
            {
                Err(TransferError::CodecNotEnabled(kind))
            }
        }
    }
}

#[cfg(feature = "raptor-q")]
fn raptor_q_ldpc_type() -> LDPCType {
    LDPCType::ReversedLDPC
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
        CodecKind::Bhp => {
            #[cfg(not(feature = "bhp"))]
            return Err(TransferError::CodecNotEnabled(kind));
            #[cfg(feature = "bhp")]
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp"))]
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
            let mut enc =
                TransferEncoderImpl::new_with_storage_manager(spec.clone(), &object, &storage)
                    .expect("encoder");
            let mut dec = TransferDecoderImpl::new_with_storage_manager(spec.clone(), &storage)
                .expect("decoder");

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
    #[cfg(feature = "bhp")]
    fn roundtrip_bhp_with_storage_manager() {
        let spec = TransferSpec::new(77, 8, CodecKind::Bhp).expect("bhp spec");
        let object = sample_object(spec.transfer_length_f);
        let mut storage = SlabStorageManager::new(spec.symbol_size_t);
        let mut enc =
            TransferEncoderImpl::new_with_storage_manager(spec.clone(), &object, &storage)
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

    #[test]
    #[cfg(feature = "raptor-10")]
    fn rejects_malformed_packet() {
        let spec = TransferSpec {
            transfer_length_f: 77,
            symbol_size_t: 8,
            codec: CodecConfig::Raptor10 { dmax: 30 },
        };
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
    fn spec_creation_rejects_zero_dimensions() {
        assert_eq!(
            TransferSpec::new(0, 10, CodecKind::RaptorQ).expect_err("empty object"),
            TransferError::InvalidTransferLength
        );
        assert_eq!(
            TransferSpec::new(10, 0, CodecKind::RaptorQ).expect_err("zero symbol size"),
            TransferError::InvalidSymbolSize
        );
    }

    #[test]
    #[cfg(feature = "raptor-q")]
    fn custom_codec_spec_is_validated_by_constructor() {
        let spec = TransferSpec::with_codec(
            10,
            2,
            CodecConfig::RaptorQ {
                dmax: DEFAULT_DMAX,
                ldpc_type: LDPCType::ReversedLDPC,
            },
        )
        .expect("valid custom codec spec");
        assert_eq!(spec.transfer_length_f, 10);
        assert_eq!(spec.codec_kind(), CodecKind::RaptorQ);
    }

    #[test]
    fn errors_explain_the_recovery_action() {
        assert_eq!(
            TransferError::DecodeNotComplete.to_string(),
            "cannot recover the object until decoding completes; add more packets"
        );
        assert_eq!(
            TransferError::SymbolLengthMismatch {
                expected: 1400,
                actual: 1200
            }
            .to_string(),
            "received a 1200-byte symbol, but the transfer expects 1400 bytes"
        );
    }

    #[test]
    #[cfg(not(any(feature = "raptor-q", feature = "raptor-10", feature = "bhp")))]
    fn disabled_codecs_return_an_actionable_error() {
        for kind in [CodecKind::RaptorQ, CodecKind::Raptor10] {
            assert_eq!(
                TransferSpec::new(10, 2, kind).expect_err("no codec feature"),
                TransferError::CodecNotEnabled(kind)
            );
        }
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
        let spec = TransferSpec {
            transfer_length_f: object.len(),
            symbol_size_t: 8,
            codec: CodecConfig::Raptor10 { dmax: 30 },
        };
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
