# fountain_transfer_core

MIT library that unifies **RaptorQ** (RFC 6330) and **Raptor10** (RFC 5053) behind a transfer-oriented encoder and decoder API.

This crate produces and consumes RFC-style packets (`sbn`, `esi`, symbol bytes) but does not perform networking or file I/O. The [`fountain_transfer`](https://github.com/shhyang/fountain_transfer/tree/main/fountain_transfer) crate adds UDP framing and a CLI.

## Dependency versions

| Crate | Version | Purpose |
|-------|---------|---------|
| [`fountain_engine`](https://docs.rs/fountain_engine) | 2.1 | Encoder, decoder, and operator traits |
| [`fountain_utility`](https://docs.rs/fountain_utility) | 2.1 | Portable vector and padding utilities |
| [`fountain_operators`](https://docs.rs/fountain_operators) | 2.0 | Slab operator adapter, with default features and SIMD disabled |
| [`fountain_scheme`](https://docs.rs/fountain_scheme) | 2.1 | RFC scheme math under `rfc::{rq,r10}` |

All of these open-source dependencies are MIT-licensed.

## Cargo features

| Feature | Default | Enables |
|---------|---------|---------|
| `raptor-q` | yes | RFC 6330 through `fountain_scheme::rfc::rq` and `CodecConfig::RaptorQ` |
| `raptor-10` | yes | RFC 5053 through `fountain_scheme::rfc::r10` and `CodecConfig::Raptor10` |

RaptorQ only:

```bash
cargo build -p fountain_transfer_core --no-default-features --features raptor-q
```

Raptor10 only:

```bash
cargo build -p fountain_transfer_core --no-default-features --features raptor-10
```

Selecting a disabled codec returns `TransferError::CodecNotEnabled`.

With neither codec feature enabled, the crate still builds for API and documentation tooling. Codec constructors return `TransferError::CodecNotEnabled`; no transfer can be encoded or decoded.

## Quick start

```rust
use fountain_transfer_core::{
    CodecKind, DecodeStatus, TransferDecoder, TransferDecoderImpl, TransferEncoder,
    TransferEncoderImpl, TransferSpec,
};

fn main() -> Result<(), fountain_transfer_core::TransferError> {
    let object: Vec<u8> = (0..199).map(|i| (i % 251) as u8).collect();
    let spec = TransferSpec::new(object.len(), 10, CodecKind::RaptorQ)?;

    let mut enc = TransferEncoderImpl::new(spec.clone(), &object)?;
    let mut dec = TransferDecoderImpl::new(spec)?;

    for packet in enc.source_packets() {
        if dec.add_packet(&packet)? == DecodeStatus::Decoded {
            break;
        }
    }
    for packet in enc.repair_packets(0, 64) {
        if dec.add_packet(&packet)? == DecodeStatus::Decoded {
            break;
        }
    }

    assert_eq!(dec.recover_object()?, object);
    Ok(())
}
```

## Configuration

`CodecConfig` is a struct-variant enum in version 2:

```rust
use fountain_transfer_core::{CodecConfig, DEFAULT_DMAX, LDPCType};

let raptor_q = CodecConfig::RaptorQ {
    dmax: DEFAULT_DMAX,
    ldpc_type: LDPCType::ReversedLDPC,
};

let raptor_10 = CodecConfig::Raptor10 {
    dmax: DEFAULT_DMAX,
};
```

`TransferSpec::new` validates its dimensions, uses `DEFAULT_DMAX` (30), and selects the default reversed LDPC precode. Use `TransferSpec::with_codec` for custom codec parameters; it applies the same validation as the convenience constructor. `LDPCType` is re-exported from `fountain_scheme::rfc::rq`.

```rust
let spec = TransferSpec::with_codec(object.len(), 1400, raptor_q)?;
```

`TransferSpec::validate()` is available when constructing a struct literal or accepting a spec from another component.

The CLI intentionally exposes only a codec choice and symbol size; these lower-level settings are library configuration.

## Main API

| Type | Role |
|------|------|
| `TransferSpec` | Object length `F`, symbol size `T`, and `CodecConfig` |
| `CodecKind` | `RaptorQ` or `Raptor10` |
| `CodecConfig` | Codec-specific `dmax`, plus RaptorQ `ldpc_type` |
| `TransferPacket` | `{ sbn, esi, symbol }` using RFC payload identity, not internal engine coded IDs |
| `TransferEncoder` / `TransferEncoderImpl` | `source_packets()` and `repair_packets(start, count)` |
| `TransferDecoder` / `TransferDecoderImpl` | `add_packet()`, `decode_status()`, and `recover_object()` |
| `StorageManager` / `SlabStorageManager` | Optional data-operator and packet storage integration |
| `DecodeStatus` | Re-exported engine decode state |

Constructors:

- `TransferEncoderImpl::new(spec, object)` uses `fountain_utility`'s vector operator.
- `TransferEncoderImpl::new_with_storage_manager(spec, object, manager)` uses the supplied operator.
- `TransferDecoderImpl::new(spec)` uses the default vector operator.
- `TransferDecoderImpl::new_with_storage_manager(spec, manager)` uses the supplied operator.

`recover_object()` returns `TransferError::DecodeNotComplete` until decoding reaches `DecodeStatus::Decoded`.

Errors describe the invalid value and the next action. For example, a disabled codec names the Cargo feature to enable, and an early recovery attempt tells the caller to add more packets. The same messages flow through the UDP helper crate as `NetError::Core`.

## RFC packet layer

RFC object layout and packet codecs remain in this crate:

| Module | Role |
|--------|------|
| `rfc6330_layout`, `rfc6330_mapping`, `rfc6330_codec` | RaptorQ object blocking, ESI mapping, `Rfc6330Encoder`, and `Rfc6330Decoder` |
| `rfc5053_layout`, `rfc5053_codec` | Raptor10 layout, `Rfc5053Encoder`, and `Rfc5053Decoder` |

The scheme implementations come from `fountain_scheme` 2.1+:

- `fountain_scheme::rfc::rq::{RaptorQSysCode, RaptorQEncoder, RaptorQDecoder, LDPCType}`
- `fountain_scheme::rfc::r10::{Raptor10SysCode, Raptor10Encoder, Raptor10Decoder}`

## Limitations

- Single source block: `sbn == 0` only.
- No session, wire, or object-transmission-information types; see `fountain_transfer` for a UDP example.
- No networking or file I/O in this crate.

## Tests

```bash
cargo test -p fountain_transfer_core
cargo clippy -p fountain_transfer_core --all-targets -- -D warnings
```

The tests cover dual-codec round trips, simulated packet loss, supplied storage managers, and malformed packet rejection.

## License

MIT. See the repository [LICENSE](https://github.com/shhyang/fountain_transfer/blob/main/LICENSE).
