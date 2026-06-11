# fountain_transfer_core

MIT library that unifies **RaptorQ** (RFC 6330) and **Raptor10** (RFC 5053) behind a single transfer-oriented API.

## Cargo features

| Feature | Default | Enables |
|---------|---------|---------|
| `raptor-q` | yes | `raptor_q` codec, [`CodecConfig::RaptorQ`] (default reversed LDPC inside the crate) |
| `raptor-10` | yes | `raptor_10` codec, [`CodecConfig::Raptor10`] |

RaptorQ only:

```bash
cargo build -p fountain_transfer_core --no-default-features --features raptor-q
```

Raptor10 only:

```bash
cargo build -p fountain_transfer_core --no-default-features --features raptor-10
```

Selecting a disabled codec returns [`TransferError::CodecNotEnabled`](src/lib.rs). It is the codec layer for [`fountain_transfer`](../fountain_transfer/) and any other front-end (tests, FLUTE bindings, custom transports).

**This crate does not perform networking or file I/O** — it produces and consumes RFC-style packets (`sbn`, `esi`, symbol bytes).

## License note

`fountain_transfer_core` is **MIT**, but it links **AGPL-3.0** [`fountain_engine`](../fountain_engine/) in-process when using the default or `fountain_operators` backends. Document that dependency in applications you ship.

## Quick start

```rust
use fountain_engine::types::DecodeStatus;
use fountain_transfer_core::{
    CodecKind, SlabStorageManager, StorageManager, TransferDecoder, TransferDecoderImpl,
    TransferEncoder, TransferEncoderImpl, TransferSpec,
};

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
```

## Public API

### Configuration

| Type | Role |
|------|------|
| [`TransferSpec`](src/lib.rs) | Object length `F`, symbol size `T`, and [`CodecConfig`](src/lib.rs); use [`TransferSpec::new`](src/lib.rs) with [`CodecKind`](src/lib.rs) |
| [`codec_config_from_kind`](src/lib.rs) | Build [`CodecConfig`] from kind (RaptorQ LDPC and degree settings are fixed in this crate) |
| [`CodecKind`](src/lib.rs) | `RaptorQ` / `Raptor10` |
| [`DecodeStatus`](src/lib.rs) | Re-exported from `fountain_engine` for decode progress |

Codec internals (including maximum degree and RaptorQ LDPC) are **not** part of the [`fountain_transfer`](../fountain_transfer/) API; that crate uses [`CodecKind`](src/lib.rs), symbol size, and [`TransferSpec::new`](src/lib.rs) only.

### Packets

| Type | Fields |
|------|--------|
| [`TransferPacket`](src/lib.rs) | `sbn`, `esi`, `symbol` — RFC payload identity, **not** engine `coded_id` |

### Encoder / decoder

| Trait | Implementations | Methods |
|-------|-----------------|---------|
| [`TransferEncoder`](src/lib.rs) | [`TransferEncoderImpl`](src/lib.rs) | `source_packets()`, `repair_packets(start, count)` |
| [`TransferDecoder`](src/lib.rs) | [`TransferDecoderImpl`](src/lib.rs) | `add_packet()`, `decode_status()`, `recover_object()` |

Constructors:

- `TransferEncoderImpl::new(spec, object)` — default in-process operator
- `TransferEncoderImpl::new_with_storage_manager(spec, object, manager)`
- `TransferDecoderImpl::new(spec)`
- `TransferDecoderImpl::new_with_storage_manager(spec, manager)`

`recover_object()` returns [`TransferError::DecodeNotComplete`](src/lib.rs) until [`DecodeStatus::Decoded`](https://docs.rs/fountain_engine) (from `fountain_engine`).

### Storage / operators

| Type | Role |
|------|------|
| [`StorageManager`](src/lib.rs) | Trait: `symbol_size()`, `new_operator()`, optional packet store |
| [`SlabStorageManager`](src/lib.rs) | Default adapter — `SlabDataOperator` or `SimdDataOperator` via `fountain_operators` |

Pass a shared `SlabStorageManager` to both encoder and decoder constructors to use the same operator backend and optionally retain packets by handle.

### Errors

[`TransferError`](src/lib.rs) covers invalid lengths, `sbn != 0` (v1), symbol size mismatch, and decode-not-complete. Malformed input should surface as `Err`, not panic.

## v1 limitations

- Single source block: **`sbn == 0` only**
- No wire/session/OTI types in this crate (see `fountain_transfer` for UDP framing)
- No `fountain_scheme` LT/LDPC path — Raptor codecs only

## Tests

```bash
cargo test -p fountain_transfer_core
```

Covers dual-codec roundtrip with `StorageManager`, simulated packet loss, and malformed-packet rejection.

## RFC packet codec (in this crate)

Transfer codecs and layout live here, not in the published scheme crates:

| Module | Role |
|--------|------|
| `rfc6330_layout`, `rfc6330_mapping`, `rfc6330_codec` | RaptorQ object blocking, ESI mapping, `Rfc6330Encoder` / `Rfc6330Decoder` |
| `rfc5053_layout`, `rfc5053_codec` | Raptor10 layout and `Rfc5053Encoder` / `Rfc5053Decoder` |

[`fountain_raptor_q`](https://github.com/wutongabc/fountain_raptor_q) and [`fountain_raptor_10`](https://github.com/wutongabc/fountain_raptor_10) supply scheme math only (`raptor_q_main`, `Raptor10SysCode`, padding wrappers). No changes are required to those published crates for transfer.

## Publishing (single GitHub repo)

This crate ships with [`fountain_transfer`](../fountain_transfer/) in one workspace:

**https://github.com/shhyang/fountain_transfer**

From the monorepo root:

```bash
./standalone/sync-from-monorepo.sh publish/fountain_transfer
```

That copies both crates, installs registry/git deps from [`Cargo.standalone.toml`](Cargo.standalone.toml), and writes the workspace root. Full steps: [standalone/PUBLISH.md](../standalone/PUBLISH.md).

For crates.io (optional), publish **`fountain_transfer_core` first**, then **`fountain_transfer`** from the same repo.

## Related

- [fountain_transfer](../fountain_transfer/) — UDP CLI (`fountain send` / `recv`)
- [raptor_q](../raptor_q/) / [raptor_10](../raptor_10/) — scheme layer (matches published `fountain_raptor_*`)
- [docs/plans/fountain-transfer.md](../docs/plans/fountain-transfer.md) — roadmap

## License

MIT. See workspace licensing notes for `fountain_engine` (AGPL-3.0 or commercial).
