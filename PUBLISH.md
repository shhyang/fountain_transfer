# Publishing fountain_transfer 2.0.0

This repository contains two MIT-licensed crates:

- `fountain_transfer_core` 2.0.0
- `fountain_transfer` 2.0.0, including the `fountain` binary

The shared open-source dependency stack is also MIT:

- `fountain_engine` 2.1
- `fountain_utility` 2.1
- `fountain_operators` 2.0
- `fountain_scheme` 2.1

RFC 6330 and RFC 5053 scheme math comes from `fountain_scheme::rfc::{rq,r10}`. No standalone Raptor package dependency is used.

## Pre-release checks

From the repository root:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p fountain_transfer --release
cargo publish -p fountain_transfer_core --dry-run --allow-dirty
```

`--allow-dirty` is only needed before the release commit exists. Run the checks again from a clean commit before publication.

The CLI's crates.io dry run becomes possible after `fountain_transfer_core` 2.0.0 has been published. Until then, its path dependency works for local builds and tests.

## Version 2 API note

`CodecConfig::RaptorQ` and `CodecConfig::Raptor10` changed from unit variants to struct variants, and the RFC transmission parameter constructors now accept codec options. Library users must update those call sites. `TransferSpec::new` remains the default constructor and does not change the positional CLI workflow.

## Publish order

After a final human review:

```bash
cargo publish -p fountain_transfer_core
cargo publish -p fountain_transfer
```

Publish `fountain_transfer_core` first and wait for the crates.io index to expose 2.0.0 before publishing the CLI.

## GitHub release

The repository-local release sequence is:

```bash
git tag v2.0.0
git push origin main
git push origin v2.0.0
```

Tags and pushes are intentionally deferred until review. In the GitHub repository settings, set the About description and topics from the root README.

## Final layout

```text
fountain_transfer/
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
├── PUBLISH.md
├── fountain_transfer_core/
│   ├── Cargo.toml
│   ├── README.md
│   └── src/
└── fountain_transfer/
    ├── Cargo.toml
    ├── README.md
    └── src/
```
