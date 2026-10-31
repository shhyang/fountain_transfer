# fountain_transfer

[![GitHub repository](https://img.shields.io/badge/GitHub-shhyang%2Ffountain__transfer-181717?logo=github)](https://github.com/shhyang/fountain_transfer)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Rateless file transfer over UDP in Rust, powered by RaptorQ and Raptor10 erasure coding.

Install the CLI from this repository, then start the receiver before the sender:

```bash
git clone https://github.com/shhyang/fountain_transfer.git
cd fountain_transfer
cargo install --path fountain_transfer

# Terminal 1: receive into received.bin
fountain recv received.bin

# Terminal 2: send a file over UDP
fountain send ./myfile.bin 127.0.0.1:7878
```

The receiver listens on `0.0.0.0:7878` by default. A future crates.io release will also support `cargo install fountain_transfer`; the crates are not yet published there.

## About this repository

Suggested GitHub description:

> Rateless UDP file transfer in Rust using RaptorQ and Raptor10 erasure coding.

Suggested GitHub topics:

`fountain-code`, `erasure-coding`, `raptorq`, `raptor-code`, `fec`, `udp`, `file-transfer`, `rust`

| Crate | Role |
|-------|------|
| [`fountain_transfer_core`](fountain_transfer_core/) | MIT library for RaptorQ/Raptor10 encoding, decoding, and RFC-style packets |
| [`fountain_transfer`](fountain_transfer/) | MIT `fountain` CLI for file transfer over UDP |

The library exposes a transfer-oriented `TransferEncoder` / `TransferDecoder` API. RaptorQ and Raptor10 packet layout stays in this repository; the underlying RFC scheme math comes from `fountain_scheme` 2.1+ under `rfc::{rq,r10}`.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p fountain_transfer --release
```

All Fountain open-source crates used by this project are MIT-licensed: `fountain_engine`, `fountain_utility`, `fountain_operators`, and `fountain_scheme`. This repository is distributed under the [MIT License](LICENSE).

## Docs

- [fountain_transfer_core/README.md](fountain_transfer_core/README.md) — library API
- [fountain_transfer/README.md](fountain_transfer/README.md) — CLI usage and wire format
- [PUBLISH.md](PUBLISH.md) — release checklist


