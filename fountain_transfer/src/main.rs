//! `fountain` CLI binary.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use fountain_transfer::{
    IgnoredDatagramReason, NetError, ProgressEvent, RecvConfig, SendConfig, SessionParams,
    codec_kind_from_cli, random_session_id, receive_object, receive_object_with_progress,
    send_object, send_object_with_progress,
};
use fountain_transfer_core::TransferError;

#[derive(Debug, Parser)]
#[command(
    name = "fountain",
    about = "Rateless UDP file transfer",
    long_about = "Send a file as RaptorQ or Raptor10 fountain-coded UDP packets.\n\
                  Start the receiver first, then run the sender.",
    version,
    arg_required_else_help = true,
    after_help = "Examples:\n  \
                  fountain recv received.bin\n  \
                  fountain send ./myfile.bin 192.168.1.20:7878\n\n\
                  UDP is connectionless: a successful send does not confirm that the receiver \
                  recovered the file."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Read a file and send fountain-coded symbols over UDP.
    Send {
        /// File to read and encode.
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Receiver endpoint, such as 192.168.1.20:7878.
        #[arg(value_name = "HOST:PORT")]
        addr: String,

        /// Codec to use: raptorq (RFC 6330) or raptor10 (RFC 5053).
        #[arg(long, default_value = "raptorq", value_name = "CODEC")]
        codec: String,

        /// Bytes in each encoded symbol; the receiver learns this value automatically.
        #[arg(
            long,
            default_value_t = 1400,
            value_name = "BYTES",
            value_parser = clap::value_parser!(u16).range(1..)
        )]
        symbol_size: u16,

        /// Repair symbols per round. Use 0 to send source symbols only.
        #[arg(long, default_value_t = 256, value_name = "COUNT")]
        repair_count: usize,

        /// Number of repair rounds. Use 0 to send source symbols only.
        #[arg(long, default_value_t = 1, value_name = "COUNT")]
        repair_rounds: usize,

        /// Fixed session identifier. Defaults to a random identifier.
        #[arg(long, value_name = "ID")]
        session_id: Option<u64>,

        /// Delay after each datagram, in milliseconds.
        #[arg(long, default_value_t = 0, value_name = "MILLISECONDS")]
        delay_ms: u64,

        /// Print periodic packet, byte, and decode progress to stderr.
        #[arg(long)]
        progress: bool,
    },

    /// Listen for symbols and write the recovered file.
    Recv {
        /// Destination file; an existing file is overwritten after decoding succeeds.
        #[arg(value_name = "PATH")]
        output: PathBuf,

        /// Local UDP address to bind.
        #[arg(long, value_name = "HOST:PORT", default_value = "0.0.0.0:7878")]
        listen: String,

        /// Accept only this session identifier. By default, use the sender's session.
        #[arg(long, value_name = "ID")]
        session_id: Option<u64>,

        /// Exit if no matching datagram arrives for this many seconds.
        #[arg(
            long,
            default_value_t = 60,
            value_name = "SECONDS",
            value_parser = clap::value_parser!(u64).range(1..)
        )]
        timeout_secs: u64,

        /// Print periodic packet, byte, and decode progress to stderr.
        #[arg(long)]
        progress: bool,
    },
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("cannot read input file `{path}`: {source}")]
    ReadInput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid --codec value: {0}")]
    InvalidCodec(String),
    #[error("invalid transfer settings: {0}")]
    TransferSettings(#[from] TransferError),
    #[error("invalid destination `{addr}`; expected HOST:PORT such as 192.168.1.20:7878: {source}")]
    ResolveAddress {
        addr: String,
        #[source]
        source: io::Error,
    },
    #[error("send to {dest} failed: {source}")]
    Send {
        dest: SocketAddr,
        #[source]
        source: NetError,
    },
    #[error(
        "receive timed out: no matching datagram arrived within {wait:?}; start `fountain recv` \
         before sending, check the sender's HOST:PORT and --session-id, or increase \
         --timeout-secs"
    )]
    ReceiveTimeout { wait: Duration },
    #[error("receive on `{listen}` failed: {source}")]
    Receive {
        listen: String,
        #[source]
        source: NetError,
    },
    #[error("cannot write output file `{path}`: {source}")]
    WriteOutput {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Commands::Send {
            path,
            addr,
            codec,
            symbol_size,
            repair_count,
            repair_rounds,
            session_id,
            delay_ms,
            progress,
        } => {
            let codec_kind = codec_kind_from_cli(&codec).map_err(CliError::InvalidCodec)?;
            let dest = resolve_addr(&addr)?;
            let object = std::fs::read(&path).map_err(|source| CliError::ReadInput {
                path: path.clone(),
                source,
            })?;
            let session_id = session_id.unwrap_or_else(random_session_id);
            let session = SessionParams::from_file_and_cli(
                session_id,
                object.len(),
                usize::from(symbol_size),
                codec_kind,
            )?;
            let config = SendConfig {
                repair_count,
                repair_rounds,
                inter_packet_delay: Duration::from_millis(delay_ms),
            };

            if progress {
                let mut printer = ProgressPrinter::default();
                let mut on_progress = |event| printer.emit(event);
                send_object_with_progress(&object, dest, &session, &config, &mut on_progress)
                    .await
                    .map_err(|source| CliError::Send { dest, source })?;
            } else {
                eprintln!(
                    "sending {} bytes from {} to {dest} (session {session_id}, codec {codec}, \
                     symbol size {symbol_size} B)",
                    object.len(),
                    path.display()
                );
                send_object(&object, dest, &session, &config)
                    .await
                    .map_err(|source| CliError::Send { dest, source })?;
                eprintln!("finished sending {} bytes to {dest}", object.len());
            }
        }
        Commands::Recv {
            output,
            listen,
            session_id,
            timeout_secs,
            progress,
        } => {
            let wait = Duration::from_secs(timeout_secs);
            eprintln!(
                "waiting on {listen} ({timeout_secs}s idle timeout), writing to {}",
                output.display()
            );
            let config = RecvConfig {
                recv_timeout: wait,
                expected_session_id: session_id,
            };
            let outcome = if progress {
                let mut printer = ProgressPrinter::default();
                let mut on_progress = |event| printer.emit(event);
                receive_object_with_progress(&listen, &config, &mut on_progress).await
            } else {
                receive_object(&listen, &config).await
            }
            .map_err(|error| map_receive_error(error, listen))?;
            write_output(&output, &outcome.object)?;
            eprintln!(
                "received {} bytes on {} from {} ({} packets)",
                outcome.object.len(),
                outcome.listen_addr,
                outcome.peer,
                outcome.packets_received
            );
        }
    }
    Ok(())
}

#[derive(Default)]
struct ProgressPrinter {
    last_decode_bucket: Option<u8>,
}

impl ProgressPrinter {
    fn emit(&mut self, event: ProgressEvent) {
        match event {
            ProgressEvent::SenderStarted {
                session_id,
                transfer_length_f,
                symbol_size_t,
                source_packets,
                repair_packets,
                planned_bytes,
            } => eprintln!(
                "progress: sending session {session_id}: {transfer_length_f} B, \
                 {symbol_size_t} B symbols, {source_packets} source + {repair_packets} repair \
                 packets, {planned_bytes} B planned"
            ),
            ProgressEvent::SenderSourcePacketSent {
                sent,
                total,
                bytes_sent,
                planned_bytes,
            } if sent == total || sent % 100 == 0 => {
                eprintln!(
                    "progress: source packets {sent}/{total}, {bytes_sent}/{planned_bytes} B"
                );
            }
            ProgressEvent::SenderRepairPacketSent {
                sent,
                total,
                bytes_sent,
                planned_bytes,
            } if sent == total || sent % 100 == 0 => {
                eprintln!(
                    "progress: repair packets {sent}/{total}, {bytes_sent}/{planned_bytes} B"
                );
            }
            ProgressEvent::SenderFinished {
                source_packets,
                repair_packets,
                bytes_sent,
                planned_bytes,
            } => eprintln!(
                "progress: send complete: {source_packets} source + {repair_packets} repair \
                 packets, {bytes_sent}/{planned_bytes} B"
            ),
            ProgressEvent::ReceiverSessionEstablished {
                session_id,
                transfer_length_f,
                symbol_size_t,
                expected_source_symbols,
            } => {
                self.last_decode_bucket = None;
                eprintln!(
                    "progress: session {session_id} established: {transfer_length_f} B, \
                     {symbol_size_t} B symbols, ~{expected_source_symbols} source symbols"
                );
            }
            ProgressEvent::ReceiverPacketAccepted {
                packets_accepted,
                bytes_accepted,
                expected_source_symbols,
                decode_percent,
            } => {
                let bucket = decode_percent / 5;
                let percent_due = bucket > self.last_decode_bucket.unwrap_or(0)
                    && decode_percent > 0
                    && decode_percent < 100;
                let packet_due = packets_accepted % 100 == 0;
                if percent_due || packet_due {
                    eprintln!(
                        "progress: receive {decode_percent}% ({packets_accepted} packets, \
                         {bytes_accepted} B accepted; ~{expected_source_symbols} source symbols)"
                    );
                }
                self.last_decode_bucket = Some(bucket);
            }
            ProgressEvent::ReceiverDecodeComplete {
                packets_accepted,
                bytes_accepted,
                ..
            } => eprintln!(
                "progress: decode complete: {packets_accepted} packets, {bytes_accepted} B accepted"
            ),
            ProgressEvent::ReceiverDatagramIgnored {
                ignored_datagrams,
                reason,
            } if ignored_datagrams == 1 || ignored_datagrams % 100 == 0 => eprintln!(
                "progress: ignored {ignored_datagrams} datagram(s) ({})",
                ignored_reason_label(reason)
            ),
            ProgressEvent::SenderSourcePacketSent { .. }
            | ProgressEvent::SenderRepairPacketSent { .. }
            | ProgressEvent::ReceiverDatagramIgnored { .. } => {}
        }
    }
}

fn ignored_reason_label(reason: IgnoredDatagramReason) -> &'static str {
    match reason {
        IgnoredDatagramReason::Malformed => "malformed",
        IgnoredDatagramReason::UnsupportedVersion => "unsupported wire version",
        IgnoredDatagramReason::UnknownCodec => "unknown codec",
        IgnoredDatagramReason::InvalidSessionMetadata => "invalid session metadata",
        IgnoredDatagramReason::BeforeSession => "packet before session metadata",
        IgnoredDatagramReason::ForeignSession => "foreign session",
        IgnoredDatagramReason::InvalidPacket => "invalid packet",
    }
}

fn map_receive_error(error: NetError, listen: String) -> CliError {
    match error {
        NetError::Timeout(wait) => CliError::ReceiveTimeout { wait },
        source => CliError::Receive { listen, source },
    }
}

fn resolve_addr(addr: &str) -> Result<SocketAddr, CliError> {
    addr.to_socket_addrs()
        .map_err(|source| CliError::ResolveAddress {
            addr: addr.to_owned(),
            source,
        })?
        .next()
        .ok_or_else(|| CliError::ResolveAddress {
            addr: addr.to_owned(),
            source: io::Error::new(
                io::ErrorKind::InvalidInput,
                "address resolved to no endpoints",
            ),
        })
}

fn write_output(path: &Path, object: &[u8]) -> Result<(), CliError> {
    std::fs::write(path, object).map_err(|source| CliError::WriteOutput {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::{Cli, CliError, Commands, resolve_addr};
    use clap::Parser;

    #[test]
    fn send_defaults_are_stable() {
        let cli = Cli::try_parse_from(["fountain", "send", "file.bin", "127.0.0.1:7878"])
            .expect("valid send arguments");
        let Commands::Send {
            codec,
            symbol_size,
            repair_count,
            repair_rounds,
            delay_ms,
            progress,
            ..
        } = cli.command
        else {
            panic!("expected send command");
        };

        assert_eq!(codec, "raptorq");
        assert_eq!(symbol_size, 1400);
        assert_eq!(repair_count, 256);
        assert_eq!(repair_rounds, 1);
        assert_eq!(delay_ms, 0);
        assert!(!progress);
    }

    #[test]
    fn rejects_zero_timeout_and_symbol_size() {
        let timeout = Cli::try_parse_from(["fountain", "recv", "out.bin", "--timeout-secs", "0"])
            .expect_err("zero timeout must be rejected");
        assert_eq!(timeout.exit_code(), 2);

        let symbol_size = Cli::try_parse_from([
            "fountain",
            "send",
            "file.bin",
            "127.0.0.1:7878",
            "--symbol-size",
            "0",
        ])
        .expect_err("zero symbol size must be rejected");
        assert_eq!(symbol_size.exit_code(), 2);
    }

    #[test]
    fn invalid_destination_has_actionable_message() {
        let error = resolve_addr("not-a-socket").expect_err("invalid address");
        let message = error.to_string();
        let CliError::ResolveAddress { addr, .. } = error else {
            panic!("expected address error");
        };
        assert_eq!(addr, "not-a-socket");
        assert!(message.contains("expected HOST:PORT"));
    }
}
