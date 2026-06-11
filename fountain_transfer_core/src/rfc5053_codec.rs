//! RFC 5053 codec layer for transfer (correctness-first, Z=1).

use crate::DEFAULT_DMAX;
use crate::rfc5053_layout;
use fountain_engine::traits::{CodeScheme, DataOperator};
use fountain_engine::{Decoder, Encoder};
use fountain_utility::VecDataOperater;
use raptor_10::Raptor10SysCode;

/// Packet identifier (single source block for now).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PayloadId {
    pub sbn: u32,
    pub esi: usize,
}

#[derive(Debug, Clone)]
pub struct EncodingPacket {
    pub payload_id: PayloadId,
    pub symbol: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Rfc5053TransmissionParams {
    pub transfer_length_f: usize,
    pub symbol_size_t: usize,
    pub k: usize,
    pub(crate) dmax: usize,
}

impl Rfc5053TransmissionParams {
    pub fn for_object(object_len_f: usize, symbol_size_t: usize) -> Self {
        assert!(symbol_size_t > 0);
        let k = rfc5053_layout::calc_k(object_len_f, symbol_size_t);
        assert!(k > 0, "object_len_f must be > 0 for now");
        Self {
            transfer_length_f: object_len_f,
            symbol_size_t,
            k,
            dmax: DEFAULT_DMAX,
        }
    }
}

/// Convenience builder matching the plan's `EncoderBuilder` shape.
pub struct Rfc5053EncoderBuilder {
    params: Rfc5053TransmissionParams,
}

impl Rfc5053EncoderBuilder {
    pub fn for_object(object_len_f: usize, symbol_size_t: usize) -> Self {
        Self {
            params: Rfc5053TransmissionParams::for_object(object_len_f, symbol_size_t),
        }
    }

    pub fn build_with_default_operator(self, object: &[u8]) -> Rfc5053Encoder {
        Rfc5053Encoder::new_with_default_operator(self.params, object)
    }

    pub fn build_with_operator(
        self,
        object: &[u8],
        operator: Box<dyn DataOperator>,
    ) -> Rfc5053Encoder {
        Rfc5053Encoder::new_with_operator(self.params, object, operator)
    }
}

pub struct Rfc5053Encoder {
    encoder: Encoder,
    params: Rfc5053TransmissionParams,
    num_total: usize,
}

impl Rfc5053Encoder {
    pub fn new_with_default_operator(
        params: Rfc5053TransmissionParams,
        object: &[u8],
    ) -> Self {
        let operator = Box::new(VecDataOperater::new(params.symbol_size_t));
        Self::new_with_operator(params, object, operator)
    }

    pub fn new_with_operator(
        params: Rfc5053TransmissionParams,
        object: &[u8],
        mut operator: Box<dyn DataOperator>,
    ) -> Self {
        assert_eq!(object.len(), params.transfer_length_f);

        let scheme = Raptor10SysCode::new(params.k, params.dmax);
        let rq_params = scheme.get_params();
        let num_total = rq_params.num_total();

        let padded = rfc5053_layout::pad_object_to_k_symbols(
            object,
            params.k,
            params.symbol_size_t,
        );

        for esi in 0..params.k {
            let start = esi * params.symbol_size_t;
            let end = start + params.symbol_size_t;
            operator.insert_vector(&padded[start..end], esi);
        }

        let encoder = Encoder::new_with_operator(&scheme, operator);
        Self {
            encoder,
            params,
            num_total,
        }
    }

    pub fn source_packets(&mut self) -> Vec<EncodingPacket> {
        (0..self.params.k)
            .map(|esi| {
                let sym = self.encoder.manager.get_coded_vector(esi);
                EncodingPacket {
                    payload_id: PayloadId { sbn: 0, esi },
                    symbol: sym,
                }
            })
            .collect()
    }

    pub fn repair_packets(
        &mut self,
        start_repair_index: usize,
        count: usize,
    ) -> Vec<EncodingPacket> {
        (0..count)
            .map(|i| {
                let coded_id = self.num_total + start_repair_index + i;
                self.encoder
                    .encode_coded_vector(coded_id)
                    .expect("repair coded_id should be encodable");
                let sym = self.encoder.manager.get_coded_vector(coded_id);
                EncodingPacket {
                    payload_id: PayloadId { sbn: 0, esi: coded_id },
                    symbol: sym,
                }
            })
            .collect()
    }
}

pub struct Rfc5053Decoder {
    decoder: Decoder,
    params: Rfc5053TransmissionParams,
}

impl Rfc5053Decoder {
    pub fn new_with_default_operator(params: Rfc5053TransmissionParams) -> Self {
        let operator = Box::new(VecDataOperater::new(params.symbol_size_t));
        Self::new_with_operator(params, operator)
    }

    pub fn new_with_operator(
        params: Rfc5053TransmissionParams,
        operator: Box<dyn DataOperator>,
    ) -> Self {
        let scheme = Raptor10SysCode::new(params.k, params.dmax);
        let decoder = Decoder::new_with_operator(&scheme, operator);
        Self { decoder, params }
    }

    pub fn add_packet(
        &mut self,
        packet: &EncodingPacket,
    ) -> fountain_engine::types::DecodeStatus {
        assert_eq!(packet.payload_id.sbn, 0, "only sbn=0 supported in v1");
        self.decoder
            .add_coded_vector(packet.payload_id.esi, &packet.symbol)
    }

    pub fn decode_status(&self) -> fountain_engine::types::DecodeStatus {
        self.decoder.decode_status()
    }

    pub fn recover_object(&self) -> Vec<u8> {
        assert_eq!(
            self.decoder.decode_status(),
            fountain_engine::types::DecodeStatus::Decoded
        );

        let mut symbols: Vec<Vec<u8>> = Vec::with_capacity(self.params.k);
        for i in 0..self.params.k {
            symbols.push(self.decoder.manager.get_data_vector(i).to_vec());
        }

        rfc5053_layout::assemble_payload_from_symbols(
            &symbols,
            self.params.transfer_length_f,
            self.params.symbol_size_t,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc5053_roundtrip_with_packet_loss() {
        let transfer_length_f = 77usize;
        let symbol_size_t = 8usize;
        let params = Rfc5053TransmissionParams::for_object(transfer_length_f, symbol_size_t);

        let object: Vec<u8> = (0..transfer_length_f).map(|i| (i % 251) as u8).collect();

        let mut enc = Rfc5053Encoder::new_with_default_operator(params.clone(), &object);
        let sources = enc.source_packets();

        let drop_idx = params.k - 1;
        let mut dec = Rfc5053Decoder::new_with_default_operator(params.clone());
        for p in &sources {
            if p.payload_id.esi == drop_idx {
                continue;
            }
            dec.add_packet(p);
        }

        let mut repair_start = 0usize;
        for _ in 0..30 {
            let repairs = enc.repair_packets(repair_start, params.k * 2);
            repair_start += params.k * 2;
            for rp in &repairs {
                if dec.add_packet(rp) == fountain_engine::types::DecodeStatus::Decoded {
                    assert_eq!(dec.recover_object(), object);
                    return;
                }
            }
        }

        panic!("failed to decode within repair budget");
    }
}
