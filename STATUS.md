# Status

Updated: 2026-10-31

## Tip

Implementation tip: `31a3d9b` (`feat(cli): improve help, status, and actionable errors`).

This status file is committed after that implementation tip; use
`git rev-parse --short HEAD` for the current final commit. Both new commits are
local on `main` and the final branch is 6 commits ahead of `origin/main`.

## Changed

- Expanded `fountain --help`, `send --help`, and `recv --help` with argument
  descriptions, defaults, endpoint examples, and the receiver-first workflow.
- Removed hidden `--addr` and `-o` rewriting. The documented positional forms
  are now the only supported CLI shape, avoiding surprising argument repair.
- Added explicit runtime exit-code handling: `0` on success, `1` for runtime or
  transfer failures, and clap's `2` for command-line usage errors.
- Replaced debug-formatted runtime errors with concise stderr messages that
  identify the failing input, destination, output, or listen address.
- Added actionable receive-timeout guidance and clarified that UDP sends do not
  acknowledge delivery.
- Added start and completion status output to stderr and removed the duplicate
  sender file read.
- Rejected zero symbol sizes and zero receive timeouts at argument parsing time;
  constrained symbol size to the wire protocol's `u16` range.
- Aligned `SendConfig::default()` pacing with the CLI's zero-delay default.
- Preserved core error types through `NetError` instead of wrapping them as
  opaque I/O errors.
- Documented public core packet, configuration, trait, constructor, and error
  types. Added `TransferSpec::with_codec` and `TransferSpec::validate`.
- Made `TransferSpec::new` validate dimensions immediately.
- Made no-codec and single-codec feature builds explicit and warning-free.
  No-codec constructors return `TransferError::CodecNotEnabled`.
- Added CLI subprocess tests for help, messages, validation, and exit codes,
  plus core validation/error tests and default-config coverage.

## Verification

- `cargo test --workspace`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `RUSTFLAGS='-D warnings' cargo clippy --workspace --no-default-features --all-targets`: passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps`: passed.
- `cargo build -p fountain_transfer --release`: passed.
- Release-binary loopback transfer: sender exit `0`, receiver exit `0`, output
  byte-for-byte identical.

## Deliberately Skipped

- No GUI. This pass was limited to CLI help, interaction, and errors.
- No networking, wire-format, session, or codec redesign.
- No per-packet percentage UI. Meaningful progress needs a callback or event API
  from the sender/receiver, which is larger than this low-risk polish pass.
- No legacy `--addr` / `-o` compatibility layer. Keeping a hidden rewrite made
  help and errors inconsistent; the positional migration is documented.
- No dependency bumps, publishing, push, remote changes, or work outside this
  publish repository.

## Remaining Gaps

1. Add opt-in transfer progress events (source/repair packet counts, bytes, and
   decode progress) so the CLI can show periodic status without tightly coupling
   the library to terminal output.
2. Harden multi-sender and malformed-datagram behavior, including clearer
   same-session reset semantics and recovery from corrupt or stale packets.
3. Add delivery acknowledgement or resume support; current UDP success means
   datagrams were accepted locally, not that the receiver completed recovery.
