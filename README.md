# fountain_transfer

Rateless UDP file transfer — **library + CLI** in one repo.

| Crate | Role |
|-------|------|
| [`fountain_transfer_core`](fountain_transfer_core/) | MIT library: dual-codec facade (RaptorQ / Raptor10), RFC packet codec, `TransferEncoder` / `TransferDecoder` |
| [`fountain_transfer`](fountain_transfer/) | MIT CLI binary `fountain`: `send` / `recv` over UDP |

**License note:** Both crates are MIT, but they link **AGPL-3.0** [`fountain_engine`](https://github.com/shhyang/fountain_engine) in-process. Document that in applications you redistribute.

## Quick start

Build the CLI from the workspace root:

```bash
cargo build -p fountain_transfer --release
```

Two terminals (loopback):

```bash
# Terminal 1
cargo run -p fountain_transfer -- recv --listen 127.0.0.1:7878 -o received.bin

# Terminal 2
cargo run -p fountain_transfer -- send ./myfile.bin --addr 127.0.0.1:7878 --codec raptorq
```

## Dependencies (published fountain crates)

This repo depends on crates.io / GitHub packages, not the full monorepo:

- `fountain_engine`, `fountain_utility` (crates.io)
- `fountain_raptor_q`, `fountain_raptor_10` (crates.io)
- `fountain_operators` ([GitHub](https://github.com/shhyang/fountain_operators))

RFC object layout and packet codecs live in **`fountain_transfer_core`**; the Raptor scheme crates are unchanged.

## Tests

```bash
cargo test --workspace
```

## Docs

- [fountain_transfer_core/README.md](fountain_transfer_core/README.md) — library API
- [fountain_transfer/README.md](fountain_transfer/README.md) — CLI flags and wire format


