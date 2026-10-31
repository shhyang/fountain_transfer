#![cfg(feature = "bhp")]

use fountain_engine::types::DecodeStatus;
use fountain_engine::DataOperator;
use fountain_transfer_core::{BhpDecoder, BhpEncoder, BhpTransmissionParams};
use fountain_utility::VecDataOperater;

#[test]
fn bhp_add_packet_all_systematic_roundtrip() {
    let transfer_length_f = 77usize;
    let symbol_size_t = 8usize;
    let params = BhpTransmissionParams::for_object(transfer_length_f, symbol_size_t);
    let object: Vec<u8> = (0..transfer_length_f).map(|i| (i % 251) as u8).collect();

    let mut enc = BhpEncoder::new_with_default_operator(params.clone(), &object);
    let sources = enc.source_packets();
    assert_eq!(sources.len(), params.k);

    let mut dec = BhpDecoder::new_with_default_operator(params.clone());
    for packet in &sources {
        let _ = dec.add_packet(packet);
    }
    assert_eq!(dec.decode_status(), DecodeStatus::Decoded);
    assert_eq!(dec.recover_object(), object);
}

#[test]
fn bhp_mark_all_systematic_resident_zero_copy() {
    let transfer_length_f = 77usize;
    let symbol_size_t = 8usize;
    let params = BhpTransmissionParams::for_object(transfer_length_f, symbol_size_t);
    let object: Vec<u8> = (0..transfer_length_f).map(|i| (i % 251) as u8).collect();

    let mut enc = BhpEncoder::new_with_default_operator(params.clone(), &object);
    let sources = enc.source_packets();

    // Pre-place source symbols into message slots, then mark resident (no add_packet copy).
    let mut op = VecDataOperater::new(symbol_size_t);
    for packet in &sources {
        op.insert_vector(&packet.symbol, packet.payload_id.esi);
    }
    let mut dec = BhpDecoder::new_with_operator(params.clone(), Box::new(op));
    assert_eq!(dec.mark_all_systematic_resident(), DecodeStatus::Decoded);
    assert_eq!(dec.recover_object(), object);
}
