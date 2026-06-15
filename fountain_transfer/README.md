# fountain_transfer

Rateless UDP file transfer CLI built on [`fountain_transfer_core`](../fountain_transfer_core/).

Published together in one repo: **https://github.com/shhyang/fountain_transfer** (see [standalone/PUBLISH.md](../standalone/PUBLISH.md)).

**License:** MIT.

## Quick start (two terminals)

Terminal 1 — receive (start first):

```bash
cargo run -p fountain_transfer --release -- recv received.bin
```

Terminal 2 — send:

```bash
cargo run -p fountain_transfer --release -- send ./myfile.bin 127.0.0.1:7878
```

Verify the transfer:

```bash
cmp ./myfile.bin received.bin && echo "OK: files match"
```

The receiver listens on `0.0.0.0:7878` by default. Codec and symbol size are sent in a `SessionMeta` frame; the receiver does not need matching flags for a basic transfer.

Long-flag forms still work for scripts: `send file --addr host:port`, `recv -o received.bin`.

## Commands

| Command | Description |
|---------|-------------|
| `send <path> <host:port>` | Encode file and emit source + repair symbols (one-way UDP) |
| `recv <output>` | Receive until decode completes, write output file |

### Common flags

| Flag | Default | Notes |
|------|---------|-------|
| `--listen` | `0.0.0.0:7878` | Recv bind address |
| `--codec` | `raptorq` | `raptorq` or `raptor10` |
| `--symbol-size` | `1400` | Bytes per fountain symbol |
| `--repair-count` | `256` | Repair symbols per round |
| `--repair-rounds` | `1` | Repeat repair generation |
| `--session-id` | random (send) | Fixed ID when filtering multiple sessions |
| `--timeout-secs` | `60` | Recv idle timeout |
| `--delay-ms` | `0` | Inter-packet delay on send |

## Wire format (v0.1)

UDP payloads are bincode-serialized [`WireMessage`](src/protocol.rs):

1. **SessionMeta** — `transfer_length_f`, `symbol_size_t`, codec, session id
2. **Packet** — `{ sbn, esi, symbol }` per RFC-style payload

## Cargo features

Forwards `fountain_transfer_core` codec features (both on by default):

| Feature | CLI impact |
|---------|------------|
| `raptor-q` | `--codec raptorq` |
| `raptor-10` | `--codec raptor10` |

Example — RaptorQ-only CLI build:

```bash
cargo build -p fountain_transfer --no-default-features --features raptor-q
```

## Tests

```bash
# All transfer tests (core + CLI)
cargo test -p fountain_transfer -p fountain_transfer_core

# Loopback UDP integration only (SHA-256 check)
cargo test -p fountain_transfer --test loopback_transfer
```

Integration tests run send and recv on `127.0.0.1` in one process and verify the recovered object byte-for-byte.

## Tips

### Session ID

Optional. Omit `--session-id` on both sides for normal use; the receiver accepts the session from `SessionMeta`. Set the same fixed id on send and recv only when filtering stray packets from earlier runs.

### Timeouts and pacing

| Situation | What to try |
|-----------|-------------|
| Recv exits with timeout | Increase `--timeout-secs` on recv |
| Large file or lossy link | Increase `--repair-count` / `--repair-rounds` on send |
| Rate limiting | `--delay-ms` on send |

### Port already in use

Change the port in both the send address and `--listen`, e.g. `127.0.0.1:7879`, or stop the previous receiver.

### Release binary

```bash
cargo build -p fountain_transfer --release
./target/release/fountain recv received.bin
./target/release/fountain send ./myfile.bin 127.0.0.1:7878
```

Use `cargo run -p fountain_transfer --release --` if you prefer not to install the binary.

### Standalone publish clone

From a synced GitHub publish tree:

```bash
cd publish/fountain_transfer   # after ./standalone/sync-from-monorepo.sh
cargo test --workspace
cargo run -p fountain_transfer --release -- recv received.bin
```

### Expected success output

```text
listening on 0.0.0.0:7878, writing to received.bin
sent 51200 bytes to 127.0.0.1:7878 (session 42424242, codec raptorq)
received 51200 bytes on 0.0.0.0:7878 from 127.0.0.1:53445 (37 packets)
```

Packet count depends on file size, symbol size, and repair settings — not on file size alone.
