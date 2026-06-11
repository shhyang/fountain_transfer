# fountain_transfer

Rateless UDP file transfer CLI built on [`fountain_transfer_core`](../fountain_transfer_core/).

Published together in one repo: **https://github.com/shhyang/fountain_transfer** (see [standalone/PUBLISH.md](../standalone/PUBLISH.md)).

**License:** MIT. 

## Quick start (two terminals)

Terminal 1 — receive (start first):

```bash
cargo run -p fountain_transfer --release -- recv \
  --listen 127.0.0.1:7878 \
  -o received.bin \
  --session-id 42424242 \
  --timeout-secs 60
```

Terminal 2 — send:

```bash
cargo run -p fountain_transfer --release -- send ./myfile.bin \
  --addr 127.0.0.1:7878 \
  --codec raptorq \
  --symbol-size 1400 \
  --repair-count 256 \
  --session-id 42424242 \
  --delay-ms 0
```

Verify the transfer:

```bash
cmp ./myfile.bin received.bin && echo "OK: files match"
```

Both sides negotiate parameters on the wire via a `SessionMeta` frame; match symbol size and repair count on lossy links. Codec choice is on the wire; RaptorQ LDPC and other codec internals are fixed inside `fountain_transfer_core`.

## Commands

| Command | Description |
|---------|-------------|
| `send <path> --addr HOST:PORT` | Encode file and emit source + repair symbols (one-way UDP) |
| `recv --listen ADDR -o PATH` | Receive until decode completes, write output file |

### Common flags

| Flag | Default | Notes |
|------|---------|-------|
| `--codec` | `raptorq` | `raptorq` or `raptor10` |
| `--symbol-size` | `1400` | Bytes per fountain symbol |
| `--repair-count` | `256` | Repair symbols per round |
| `--repair-rounds` | `1` | Repeat repair generation |
| `--session-id` | random | Fixed ID for testing |

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

For manual two-terminal tests, pass the **same** `--session-id` on send and recv. The sender defaults to a random id each run; without a fixed id on recv, stray packets from an earlier run can confuse debugging.

### Timeouts and pacing

| Situation | What to try |
|-----------|-------------|
| Recv exits with timeout | Increase `--timeout-secs` on recv |
| Large file or slow link | Increase `--repair-count` / `--repair-rounds` on send |
| Artificial pacing | `--delay-ms` on send (default `2`; use `0` on loopback) |

### Port already in use

Change the port in both `--listen` and `--addr`, e.g. `127.0.0.1:7879`, or stop the previous receiver.

### Release binary

```bash
cargo build -p fountain_transfer --release
./target/release/fountain send ...   # or recv ...
```

Use `cargo run -p fountain_transfer --release --` if you prefer not to install the binary.

### Standalone publish clone

From a synced GitHub publish tree:

```bash
cd publish/fountain_transfer   # after ./standalone/sync-from-monorepo.sh
cargo test --workspace
cargo run -p fountain_transfer --release -- recv ...
```

### Expected success output

```text
sent 51200 bytes to 127.0.0.1:7878 (session 42424242, codec raptorq)
received 51200 bytes from 127.0.0.1:53445 (37 packets)
```

Packet count depends on file size, symbol size, and repair settings — not on file size alone.

