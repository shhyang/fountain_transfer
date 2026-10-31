# fountain_transfer

Rateless UDP file transfer CLI built on [`fountain_transfer_core`](https://github.com/shhyang/fountain_transfer/tree/main/fountain_transfer_core).

Source and release notes: **https://github.com/shhyang/fountain_transfer**.

**License:** MIT.

## Install and run

From the repository root:

```bash
cargo install --path fountain_transfer
```

Start the receiver first, then run the sender in another terminal:

Terminal 1 — receive (start first):

```bash
fountain recv received.bin
```

Terminal 2 — send:

```bash
fountain send ./myfile.bin 127.0.0.1:7878
```

Verify the transfer:

```bash
cmp ./myfile.bin received.bin && echo "OK: files match"
```

The receiver listens on `0.0.0.0:7878` by default. Codec and symbol size are sent in a `SessionMeta` frame; the receiver does not need matching flags for a basic transfer.

The command line uses positional paths and destinations: `send <path> <host:port>` and `recv <output>`. `cargo install fountain_transfer` will be available when the crates are published to crates.io.

The legacy `--addr` and `-o` forms are no longer rewritten into positionals. Use the forms above so argument errors and help text stay consistent.

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
| `--symbol-size` | `1400` | Bytes per fountain symbol; must be at least 1 |
| `--repair-count` | `256` | Repair symbols per round; `0` sends source symbols only |
| `--repair-rounds` | `1` | Number of repair rounds; `0` sends source symbols only |
| `--session-id` | random (send) | Fixed ID when filtering multiple sessions |
| `--timeout-secs` | `60` | Recv idle timeout; must be at least 1 |
| `--delay-ms` | `0` | Inter-packet delay on send |

## Status and exit codes

Both commands write progress to stderr, leaving stdout available for scripting. Runtime validation and transfer failures exit with status `1`. Invalid command-line syntax exits with status `2`, matching clap conventions. Successful completion exits with status `0`.

CLI errors identify the failing input, destination, or output path. Receive timeouts suggest checking startup order, ports, session IDs, and `--timeout-secs`.

UDP does not acknowledge delivery. `finished sending` means the local socket accepted every datagram; confirm recovery by checking the receiver's completion line and the output file.

## Wire format (version 1)

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

With neither codec feature enabled, the CLI still builds for documentation and error-path tooling, but `send` fails for either codec with an instruction naming the missing Cargo feature.

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

### Dependencies and licensing

From the repository root:

```bash
cargo test --workspace
cargo run -p fountain_transfer --release -- recv received.bin
```

The library and CLI depend on the MIT-licensed `fountain_engine` 2.1, `fountain_utility` 2.1, `fountain_operators` 2.0, and `fountain_scheme` 2.1 crates. RaptorQ and Raptor10 scheme math is provided by `fountain_scheme::rfc::{rq,r10}`.

### Expected success output

```text
waiting on 0.0.0.0:7878 (60s idle timeout), writing to received.bin
sending 51200 bytes from ./myfile.bin to 127.0.0.1:7878 (session 42424242, codec raptorq, symbol size 1400 B)
finished sending 51200 bytes to 127.0.0.1:7878
received 51200 bytes on 0.0.0.0:7878 from 127.0.0.1:53445 (37 packets)
```

Packet count depends on file size, symbol size, and repair settings — not on file size alone.
