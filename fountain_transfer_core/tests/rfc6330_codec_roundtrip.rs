#![cfg(feature = "raptor-q")]

use fountain_engine::types::DecodeStatus;
use fountain_transfer_core::{
    rfc6330_layout, Rfc6330Decoder, Rfc6330Encoder, Rfc6330TransmissionParams,
};

#[test]
fn rfc6330_codec_roundtrip() {
    let transfer_length_f = 199usize;
    let symbol_size_t = 10usize;
    let k = rfc6330_layout::calc_k(transfer_length_f, symbol_size_t);
    assert_eq!(k, 20);

    let params = Rfc6330TransmissionParams::for_object(transfer_length_f, symbol_size_t);

    let object: Vec<u8> = (0..transfer_length_f).map(|i| (i % 251) as u8).collect();

    let mut enc = Rfc6330Encoder::new_with_default_operator(params.clone(), &object);
    let sources = enc.source_packets();
    assert_eq!(sources.len(), params.k);

    let mut dec = Rfc6330Decoder::new_with_default_operator(params.clone());
    for packet in &sources {
        let _ = dec.add_packet(packet);
    }
    assert_eq!(dec.decode_status(), DecodeStatus::Decoded);
    assert_eq!(dec.recover_object(), object);
}

#[test]
fn rfc6330_k_prime_matches_encoder() {
    let transfer_length_f = 199usize;
    let symbol_size_t = 10usize;
    let object: Vec<u8> = (0..transfer_length_f).map(|i| (i % 251) as u8).collect();
    let params = Rfc6330TransmissionParams::for_object(transfer_length_f, symbol_size_t);
    let enc = Rfc6330Encoder::new_with_default_operator(params, &object);
    assert!(enc.k_prime() >= rfc6330_layout::calc_k(transfer_length_f, symbol_size_t));
}
