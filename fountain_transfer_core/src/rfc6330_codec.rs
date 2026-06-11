//! Ref-like (but correctness-first) RFC6330 codec layer for transfer.
//!
//! This initial v1 API is intentionally small:
//! - only supports Z=1 blocking (single source block)
//! - exposes packet ingest for systematic + repair packets
//! - delegates padding (K … K′−1) to [`raptor_q::RaptorQEncoder`] / [`raptor_q::RaptorQDecoder`]

use crate::{default_raptor_q_ldpc_type, DEFAULT_DMAX};
use crate::rfc6330_layout;
use crate::rfc6330_mapping;
use fountain_engine::traits::{CodeScheme, DataOperator};
use fountain_utility::VecDataOperater;
use raptor_q::raptor_q_main::raptor_q_main;
use raptor_q::{LDPCType, RaptorQDecoder, RaptorQEncoder};

/// Packet identifier (single source block for now).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PayloadId {
    pub sbn: u32,   // source block number
    pub esi: usize, // encoding symbol id
}

/// A single encoding packet.
#[derive(Debug, Clone)]
pub struct EncodingPacket {
    pub payload_id: PayloadId,
    pub symbol: Vec<u8>, // fixed-size symbol bytes
}

/// Parameters for RFC6330 object transmission (v1).
#[derive(Debug, Clone)]
pub struct Rfc6330TransmissionParams {
    /// Transfer length `F` in bytes.
    pub transfer_length_f: usize,
    /// Symbol size `T` in bytes.
    pub symbol_size_t: usize,
    /// Source symbol count `K` derived as `ceil(F/T)`.
    pub k: usize,
    pub(crate) dmax: usize,
    pub(crate) ldpc_type: LDPCType,
}

/// Convenience builder matching the plan's `EncoderBuilder` shape.
pub struct Rfc6330EncoderBuilder {
    params: Rfc6330TransmissionParams,
}

impl Rfc6330EncoderBuilder {
    pub fn for_object(object_len_f: usize, symbol_size_t: usize) -> Self {
        Self {
            params: Rfc6330TransmissionParams::for_object(object_len_f, symbol_size_t),
        }
    }

    pub fn build_with_default_operator(self, object: &[u8]) -> Rfc6330Encoder {
        Rfc6330Encoder::new_with_default_operator(self.params, object)
    }

    pub fn build_with_operator(
        self,
        object: &[u8],
        operator: Box<dyn DataOperator>,
    ) -> Rfc6330Encoder {
        Rfc6330Encoder::new_with_operator(self.params, object, operator)
    }
}

impl Rfc6330TransmissionParams {
    /// Derives `K` as `ceil(F/T)` using the transfer crate's default RaptorQ settings.
    pub fn for_object(object_len_f: usize, symbol_size_t: usize) -> Self {
        Self::with_codec_options(
            object_len_f,
            symbol_size_t,
            DEFAULT_DMAX,
            default_raptor_q_ldpc_type(),
        )
    }

    pub(crate) fn with_codec_options(
        object_len_f: usize,
        symbol_size_t: usize,
        dmax: usize,
        ldpc_type: LDPCType,
    ) -> Self {
        let k = rfc6330_layout::calc_k(object_len_f, symbol_size_t);
        assert!(k > 0, "object_len_f must be > 0 for now");
        assert!(symbol_size_t > 0, "symbol_size_t must be > 0");
        Self {
            transfer_length_f: object_len_f,
            symbol_size_t,
            k,
            dmax,
            ldpc_type,
        }
    }
}

pub struct Rfc6330Encoder {
    encoder: RaptorQEncoder,
    params: Rfc6330TransmissionParams,
    num_total: usize,
}

impl Rfc6330Encoder {
    pub fn new_with_default_operator(
        params: Rfc6330TransmissionParams,
        object: &[u8],
    ) -> Self {
        let operator = Box::new(VecDataOperater::new(params.symbol_size_t));
        Self::new_with_operator(params, object, operator)
    }

    pub fn new_with_operator(
        params: Rfc6330TransmissionParams,
        object: &[u8],
        mut operator: Box<dyn DataOperator>,
    ) -> Self {
        assert_eq!(object.len(), params.transfer_length_f);

        let scheme = raptor_q_main::new(
            params.k,
            params.dmax,
            params.ldpc_type.clone(),
        );
        let num_total = scheme.get_params().num_total();

        let padded_k = rfc6330_layout::pad_object_to_k_symbols(
            object,
            params.k,
            params.symbol_size_t,
        );
        for esi in 0..params.k {
            let start = esi * params.symbol_size_t;
            let end = start + params.symbol_size_t;
            operator.insert_vector(&padded_k[start..end], esi);
        }

        let encoder = RaptorQEncoder::new_with_operator(
            scheme,
            operator,
            params.symbol_size_t,
        );
        Self {
            encoder,
            params,
            num_total,
        }
    }

    /// Returns the systematic packets for ESI in `[0, K)`.
    pub fn source_packets(&mut self) -> Vec<EncodingPacket> {
        (0..self.params.k)
            .map(|esi| {
                let sym = self.encoder.inner.manager.get_coded_vector(esi);
                EncodingPacket {
                    payload_id: PayloadId { sbn: 0, esi },
                    symbol: sym,
                }
            })
            .collect()
    }

    /// Returns `count` repair packets with RFC6330 repair ESIs `K + start + i`.
    pub fn repair_packets(
        &mut self,
        start_repair_index: usize,
        count: usize,
    ) -> Vec<EncodingPacket> {
        (0..count)
            .map(|i| {
                let repair_index = start_repair_index + i;
                let coded_id = self.num_total + repair_index;
                let esi =
                    rfc6330_mapping::repair_payload_esi(self.params.k, repair_index);
                self.encoder
                    .encode_coded_vector(coded_id)
                    .expect("repair coded_id should be encodable");
                let sym = self.encoder.inner.manager.get_coded_vector(coded_id);
                EncodingPacket {
                    payload_id: PayloadId { sbn: 0, esi },
                    symbol: sym,
                }
            })
            .collect()
    }

    pub fn k_prime(&self) -> usize {
        self.encoder.block_symbols()
    }

    /// RFC6330 padding symbol count (`K′ − K`).
    pub fn num_padding(&self) -> usize {
        rfc6330_mapping::num_padding_symbols(self.params.k, self.k_prime())
    }
}

pub struct Rfc6330Decoder {
    decoder: RaptorQDecoder,
    params: Rfc6330TransmissionParams,
    num_total: usize,
}

impl Rfc6330Decoder {
    pub fn new_with_default_operator(params: Rfc6330TransmissionParams) -> Self {
        let operator = Box::new(VecDataOperater::new(params.symbol_size_t));
        Self::new_with_operator(params, operator)
    }

    pub fn new_with_operator(
        params: Rfc6330TransmissionParams,
        operator: Box<dyn DataOperator>,
    ) -> Self {
        let scheme = raptor_q_main::new(
            params.k,
            params.dmax,
            params.ldpc_type.clone(),
        );
        let num_total = scheme.get_params().num_total();
        let decoder = RaptorQDecoder::new_with_operator(
            scheme,
            operator,
            params.symbol_size_t,
        );
        Self {
            decoder,
            params,
            num_total,
        }
    }

    pub fn add_packet(&mut self, packet: &EncodingPacket) -> fountain_engine::types::DecodeStatus {
        assert_eq!(packet.payload_id.sbn, 0, "only sbn=0 supported in v1");
        let coded_id = rfc6330_mapping::payload_esi_to_coded_id(
            packet.payload_id.esi,
            self.params.k,
            self.num_total,
        );
        self.decoder.add_coded_vector(coded_id, &packet.symbol)
    }

    pub fn decode_status(&self) -> fountain_engine::types::DecodeStatus {
        self.decoder.decode_status()
    }

    /// Recovers the original object bytes (truncated to `F`).
    ///
    /// Note: this assumes decoding has completed.
    pub fn recover_object(&self) -> Vec<u8> {
        assert_eq!(
            self.decoder.decode_status(),
            fountain_engine::types::DecodeStatus::Decoded
        );

        let mut symbols: Vec<Vec<u8>> = Vec::with_capacity(self.params.k);
        for i in 0..self.params.k {
            symbols.push(self.decoder.get_data_vector(i).to_vec());
        }

        rfc6330_layout::assemble_payload_from_symbols(
            &symbols,
            self.params.transfer_length_f,
            self.params.symbol_size_t,
        )
    }

    pub fn k_prime(&self) -> usize {
        self.decoder.block_symbols()
    }

    /// RFC6330 padding symbol count (`K′ − K`).
    pub fn num_padding(&self) -> usize {
        rfc6330_mapping::num_padding_symbols(self.params.k, self.k_prime())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6330_roundtrip_with_packet_loss() {
        let transfer_length_f = 199usize;
        let symbol_size_t = 10usize;
        let k = rfc6330_layout::calc_k(transfer_length_f, symbol_size_t);
        assert_eq!(k, 20);

        let params = Rfc6330TransmissionParams::for_object(
            transfer_length_f,
            symbol_size_t,
        );

        let object: Vec<u8> = (0..transfer_length_f).map(|i| (i % 251) as u8).collect();

        let mut enc = Rfc6330Encoder::new_with_default_operator(params.clone(), &object);
        let all_source_packets = enc.source_packets();

        let drop_idx = params.k - 1;
        let mut dec = Rfc6330Decoder::new_with_default_operator(params.clone());
        for p in all_source_packets.iter() {
            if p.payload_id.esi == drop_idx {
                continue;
            }
            let _ = dec.add_packet(p);
        }

        let mut repair_start = 0usize;
        for _round in 0..20 {
            let repairs = enc.repair_packets(repair_start, params.k * 2);
            repair_start += params.k * 2;
            for rp in &repairs {
                if dec.add_packet(rp) == fountain_engine::types::DecodeStatus::Decoded {
                    let recovered = dec.recover_object();
                    assert_eq!(recovered, object);
                    return;
                }
            }
        }

        panic!("failed to decode with the chosen repair search budget");
    }
}
