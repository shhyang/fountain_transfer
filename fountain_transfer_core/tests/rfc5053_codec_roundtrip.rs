#![cfg(feature = "raptor-10")]

use fountain_engine::types::DecodeStatus;
use fountain_transfer_core::{Rfc5053Decoder, Rfc5053Encoder, Rfc5053TransmissionParams};

#[test]
fn rfc5053_codec_roundtrip() {
    let transfer_length_f = 77usize;
    let symbol_size_t = 8usize;
    let params = Rfc5053TransmissionParams::for_object(transfer_length_f, symbol_size_t);

    let object: Vec<u8> = (0..transfer_length_f).map(|i| (i % 251) as u8).collect();

    let mut enc = Rfc5053Encoder::new_with_default_operator(params.clone(), &object);
    let sources = enc.source_packets();
    assert_eq!(sources.len(), params.k);

    let mut dec = Rfc5053Decoder::new_with_default_operator(params.clone());
    for packet in &sources {
        let _ = dec.add_packet(packet);
    }
    assert_eq!(dec.decode_status(), DecodeStatus::Decoded);
    assert_eq!(dec.recover_object(), object);
}
