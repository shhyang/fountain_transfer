//! RFC 6330 layout helpers (v1).
//!
//! This is a correctness-first "blocking + tail padding + payload assembly" layer.
//! It is intentionally minimal for now (Z=1, no sub-block reshape) so it can
//! delegate the actual encoding/decoding math to `fountain_engine`.

/// Number of source symbols (`K`) for an object of length `F` bytes and symbol size `T` bytes.
#[inline]
pub fn calc_k(transfer_length_f: usize, symbol_size_t: usize) -> usize {
    assert!(symbol_size_t > 0, "symbol_size_t must be > 0");
    transfer_length_f.div_ceil(symbol_size_t)
}

/// Pads `object` with zeroes up to `k * symbol_size`, returning the padded symbol material.
pub fn pad_object_to_k_symbols(object: &[u8], k: usize, symbol_size: usize) -> Vec<u8> {
    assert!(
        k == 0 || object.len() <= k * symbol_size,
        "object.len() must be <= k * symbol_size"
    );

    let mut padded = Vec::with_capacity(k * symbol_size);
    padded.extend_from_slice(object);

    let target_len = k * symbol_size;
    if padded.len() < target_len {
        padded.resize(target_len, 0u8);
    }
    padded
}

/// Padded source-block bytes for Z=1 (tail zero-fill to `k * T`), matching `ref/raptorq` block prep.
pub fn padded_object_z1(
    object: &[u8],
    transfer_length_f: usize,
    symbol_size_t: usize,
) -> Vec<u8> {
    assert_eq!(object.len(), transfer_length_f);
    let k = calc_k(transfer_length_f, symbol_size_t);
    pad_object_to_k_symbols(object, k, symbol_size_t)
}

/// Splits padded symbol material into `k` fixed-size symbols.
pub fn split_into_symbols(padded: &[u8], k: usize, symbol_size: usize) -> Vec<&[u8]> {
    assert_eq!(padded.len(), k * symbol_size);
    (0..k)
        .map(|i| &padded[i * symbol_size..(i + 1) * symbol_size])
        .collect()
}

/// Concatenates `k` fixed-size symbols and truncates to `transfer_length_f`.
pub fn assemble_payload_from_symbols(
    symbols: &[Vec<u8>],
    transfer_length_f: usize,
    symbol_size: usize,
) -> Vec<u8> {
    assert!(!symbols.is_empty());
    assert_eq!(symbols[0].len(), symbol_size);

    let mut out = Vec::with_capacity(transfer_length_f);
    for sym in symbols {
        out.extend_from_slice(sym);
    }
    out.truncate(transfer_length_f);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_k_works() {
        assert_eq!(calc_k(0, 8), 0);
        assert_eq!(calc_k(1, 8), 1);
        assert_eq!(calc_k(8, 8), 1);
        assert_eq!(calc_k(9, 8), 2);
    }

    #[test]
    fn tail_padding_and_trim_roundtrip() {
        let transfer_length_f = 19;
        let symbol_size = 8;
        let k = calc_k(transfer_length_f, symbol_size);
        assert_eq!(k, 3);

        let object: Vec<u8> = (0..transfer_length_f).map(|i| (i % 251) as u8).collect();
        let padded = pad_object_to_k_symbols(&object, k, symbol_size);
        assert_eq!(padded.len(), k * symbol_size);
        assert_eq!(&padded[..transfer_length_f], object.as_slice());

        let symbols: Vec<Vec<u8>> = (0..k)
            .map(|i| padded[i * symbol_size..(i + 1) * symbol_size].to_vec())
            .collect();
        let recovered = assemble_payload_from_symbols(&symbols, transfer_length_f, symbol_size);
        assert_eq!(recovered, object);
    }
}

