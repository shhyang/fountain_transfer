# Status

Updated: 2026-10-31

## Tip

Implementation tip: `b4624a6` (`feat(core/cli): add progress events and harden datagrams`).
Full SHA: `b4624a64aff5b1b561a2c9bdc7b4886292807007`.

This status file is committed after that implementation tip; use
`git rev-parse --short HEAD` for the final documentation commit. The status
commit brings the local `main` branch to 8 commits ahead of `origin/main`.

## Changed in this pass

- Added public `ProgressEvent` and `ProgressCallback` APIs. Sender events cover
  session start, every source/repair packet with cumulative and planned bytes,
  and send completion. Receiver events cover session establishment, accepted
  packets and bytes, approximate decode percentage, completion, and ignored
  datagrams.
- Added `send_*_with_progress` and `receive_*_with_progress` variants while
  preserving all existing callback-free API signatures and default CLI output.
- Documented and implemented decode percentage as
  `min(99, 100 * packets_accepted / ceil(transfer_length_f / symbol_size_t))`;
  it is exactly `100` only after codec decoding completes.
- Added `--progress` to both CLI commands. Status is written to stderr and
  throttled to every 100 packets plus roughly 5% decode bands, with start and
  finish events always reported.
- Hardened receive so malformed, undecodable, wrong-version, unknown-codec,
  wrong-session, invalid-symbol, and unexpected-SBN datagrams are skipped
  instead of failing the transfer.
- Packets before metadata remain ignored. With no expected session ID, the
  receiver now sticks to the first valid session and ignores foreign metadata
  and packets until completion or timeout.
- A repeated metadata frame with the active session ID now intentionally resets
  decoder state and accepted packet/byte counters so a full retransmission can
  recover.
- Added loopback coverage for progress events, corrupt-datagram recovery,
  foreign-session isolation, and same-session retransmission/reset. Added CLI
  help coverage for both `--progress` flags.

## Previous local changes

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

- `cargo test --workspace`: **passed** (all unit, CLI subprocess, loopback,
  codec integration, and doc tests).
- `cargo clippy --workspace --all-targets -- -D warnings`: **passed**.

## Deliberately Skipped

- Delivery acknowledgement and resume support remain deliberately skipped.
- No GUI.
- No wire-format/version bump; `WIRE_VERSION` remains `1`.
- No legacy `--addr` / `-o` compatibility layer. Keeping a hidden rewrite made
  help and errors inconsistent; the positional migration is documented.
- No dependency bumps, publishing, push, remote changes, or work outside this
  publish repository.

## Remaining Gaps

- Delivery acknowledgement and resume are not implemented. Current UDP success
  means datagrams were accepted locally, not that the receiver recovered.
