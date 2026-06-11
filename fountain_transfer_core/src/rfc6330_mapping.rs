//! RFC 6330 payload ID mapping (wire ESI ↔ `fountain_engine` coded IDs).
//!
//! See `ref/raptorq` `SourceBlockEncoder::repair_packets` and `SourceBlockDecoder`:
//! - Source ESIs: `0..K`
//! - Repair ESIs: `K, K+1, …` (K = source symbols in the block, not K′)
//! - Internal tuple / ISI for repair index `j` uses `K′ + j` (handled inside degree-set)

/// Number of K′−K padding symbols (RFC6330 §5.3.1).
#[inline]
pub fn num_padding_symbols(source_k: usize, k_prime: usize) -> usize {
    k_prime.saturating_sub(source_k)
}

/// Map a wire [`PayloadId`](super::rfc6330_codec::PayloadId) ESI to engine `coded_id`.
///
/// - `esi < K` → systematic source (`coded_id = esi`)
/// - `esi >= K` → repair (`coded_id = num_total + (esi - K)`)
#[inline]
pub fn payload_esi_to_coded_id(esi: usize, source_k: usize, num_total: usize) -> usize {
    if esi < source_k {
        esi
    } else {
        num_total + repair_index_from_payload_esi(esi, source_k)
    }
}

/// RFC6330 repair packet ESI for repair index `repair_index` (0-based).
#[inline]
pub fn repair_payload_esi(source_k: usize, repair_index: usize) -> usize {
    source_k + repair_index
}

/// Repair index from RFC repair ESI (panics if `esi < source_k`).
#[inline]
pub fn repair_index_from_payload_esi(esi: usize, source_k: usize) -> usize {
    assert!(esi >= source_k, "esi {esi} is not a repair ESI for K={source_k}");
    esi - source_k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_repair_esi_mapping() {
        let k = 20;
        let k_prime = 24;
        assert_eq!(num_padding_symbols(k, k_prime), 4);

        let num_total = 50;
        for repair_index in 0..5 {
            let esi = repair_payload_esi(k, repair_index);
            assert_eq!(esi, k + repair_index);
            assert_eq!(repair_index_from_payload_esi(esi, k), repair_index);
            let coded = payload_esi_to_coded_id(esi, k, num_total);
            assert_eq!(coded, num_total + repair_index);
        }
    }
}
