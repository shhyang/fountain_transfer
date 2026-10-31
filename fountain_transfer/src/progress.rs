//! Opt-in progress events shared by senders, receivers, and front ends.

/// Callback used by the `_with_progress` transfer APIs.
///
/// The callback is invoked synchronously as events occur. Requiring `Send`
/// keeps the returned transfer futures usable with multi-threaded executors.
pub type ProgressCallback<'a> = dyn FnMut(ProgressEvent) + Send + 'a;

/// Why a receiver ignored a datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoredDatagramReason {
    /// The UDP payload could not be decoded as a wire message.
    Malformed,
    /// The payload used an unsupported wire version.
    UnsupportedVersion,
    /// Session metadata named a codec that this build does not recognize.
    UnknownCodec,
    /// Session metadata contained invalid transfer dimensions.
    InvalidSessionMetadata,
    /// A packet arrived before any session was established.
    BeforeSession,
    /// The datagram belonged to a different session.
    ForeignSession,
    /// The packet failed packet validation, including an unexpected source block.
    InvalidPacket,
}

/// Progress reported by transfer senders and receivers.
///
/// Receiver decode percentages are estimates. Before decoding completes they
/// use `min(99, 100 * packets_accepted / expected_source_symbols)`, where
/// `expected_source_symbols` is `ceil(transfer_length_f / symbol_size_t)`.
/// Repair packets and duplicates can advance this estimate without contributing
/// a new source symbol. The percentage is exactly `100` only after the codec
/// reports that decoding is complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    /// A sender finished encoding and is about to send session metadata.
    SenderStarted {
        /// Transfer session identifier.
        session_id: u64,
        /// Original object length in bytes.
        transfer_length_f: usize,
        /// Encoded symbol size in bytes.
        symbol_size_t: usize,
        /// Planned source packets.
        source_packets: usize,
        /// Planned repair packets across all configured rounds.
        repair_packets: usize,
        /// Planned UDP payload bytes, including session metadata and bincode framing.
        planned_bytes: usize,
    },
    /// A source packet was sent.
    SenderSourcePacketSent {
        /// Source packets sent so far.
        sent: usize,
        /// Total planned source packets.
        total: usize,
        /// UDP payload bytes sent so far, including session metadata.
        bytes_sent: usize,
        /// Total planned UDP payload bytes.
        planned_bytes: usize,
    },
    /// A repair packet was sent.
    SenderRepairPacketSent {
        /// Repair packets sent so far.
        sent: usize,
        /// Total planned repair packets.
        total: usize,
        /// UDP payload bytes sent so far, including session metadata.
        bytes_sent: usize,
        /// Total planned UDP payload bytes.
        planned_bytes: usize,
    },
    /// All configured datagrams were accepted by the local UDP socket.
    SenderFinished {
        /// Source packets sent.
        source_packets: usize,
        /// Repair packets sent.
        repair_packets: usize,
        /// UDP payload bytes sent.
        bytes_sent: usize,
        /// Total planned UDP payload bytes.
        planned_bytes: usize,
    },
    /// A sender's session metadata established the receiver's decoder.
    ReceiverSessionEstablished {
        /// Transfer session identifier.
        session_id: u64,
        /// Original object length in bytes.
        transfer_length_f: usize,
        /// Encoded symbol size in bytes.
        symbol_size_t: usize,
        /// Approximate number of source symbols needed to recover the object.
        expected_source_symbols: usize,
    },
    /// A structurally valid packet was accepted by the decoder.
    ReceiverPacketAccepted {
        /// Valid packets accepted in the current session, including duplicates.
        packets_accepted: usize,
        /// Encoded symbol bytes accepted in the current session.
        bytes_accepted: usize,
        /// Approximate number of source symbols needed to recover the object.
        expected_source_symbols: usize,
        /// Approximate decode progress from 0 through 99, or 100 when complete.
        decode_percent: u8,
    },
    /// The decoder reached [`fountain_transfer_core::DecodeStatus::Decoded`].
    ReceiverDecodeComplete {
        /// Valid packets accepted in the completed session.
        packets_accepted: usize,
        /// Encoded symbol bytes accepted in the completed session.
        bytes_accepted: usize,
        /// Always `100` for this event.
        decode_percent: u8,
    },
    /// A malformed, stale, or foreign datagram was skipped without failing receive.
    ReceiverDatagramIgnored {
        /// Datagrams ignored by this receiver so far.
        ignored_datagrams: usize,
        /// Reason this datagram was ignored.
        reason: IgnoredDatagramReason,
    },
}

pub(crate) fn decode_percent(
    packets_accepted: usize,
    expected_source_symbols: usize,
    decoded: bool,
) -> u8 {
    if decoded {
        return 100;
    }
    let percent = packets_accepted
        .saturating_mul(100)
        .checked_div(expected_source_symbols)
        .unwrap_or(0);
    percent.min(99) as u8
}

#[cfg(test)]
mod tests {
    use super::decode_percent;

    #[test]
    fn decode_percentage_is_capped_until_decoded() {
        assert_eq!(decode_percent(0, 10, false), 0);
        assert_eq!(decode_percent(1, 10, false), 10);
        assert_eq!(decode_percent(20, 10, false), 99);
        assert_eq!(decode_percent(20, 10, true), 100);
        assert_eq!(decode_percent(1, 0, false), 0);
    }
}
