//! `fountain` CLI binary.

use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use fountain_transfer::{
    codec_kind_from_cli, random_session_id, receive_to_file, send_file, RecvConfig, SendConfig,
    SessionParams,
};

#[derive(Parser)]
#[command(name = "fountain", about = "Rateless UDP file transfer", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Read a file and send fountain-coded symbols over UDP.
    Send {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(value_name = "HOST:PORT")]
        addr: String,
        #[arg(long, default_value = "raptorq")]
        codec: String,
        #[arg(long, default_value_t = 1400)]
        symbol_size: usize,
        #[arg(long, default_value_t = 256)]
        repair_count: usize,
        #[arg(long, default_value_t = 1)]
        repair_rounds: usize,
        #[arg(long)]
        session_id: Option<u64>,
        #[arg(long, default_value_t = 0)]
        delay_ms: u64,
    },
    /// Listen for symbols and write the recovered object.
    Recv {
        #[arg(value_name = "PATH")]
        output: PathBuf,
        #[arg(long, value_name = "HOST:PORT", default_value = "0.0.0.0:7878")]
        listen: String,
        #[arg(long)]
        session_id: Option<u64>,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse_from(normalize_cli_args(std::env::args().collect()));
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
        } => {
            let object = std::fs::read(&path)?;
            let codec_kind = codec_kind_from_cli(&codec).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
            })?;
            let session_id = session_id.unwrap_or_else(random_session_id);
            let session = SessionParams::from_file_and_cli(
                session_id,
                object.len(),
                symbol_size,
                codec_kind,
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Unsupported, e.to_string()))?;
            let dest = resolve_addr(&addr)?;
            let config = SendConfig {
                repair_count,
                repair_rounds,
                inter_packet_delay: Duration::from_millis(delay_ms),
            };
            send_file(&path, dest, &session, &config).await?;
            eprintln!(
                "sent {} bytes to {dest} (session {session_id}, codec {codec})",
                object.len()
            );
        }
        Commands::Recv {
            output,
            listen,
            session_id,
            timeout_secs,
        } => {
            eprintln!(
                "listening on {listen}, writing to {}",
                output.display()
            );
            let config = RecvConfig {
                recv_timeout: Duration::from_secs(timeout_secs),
                expected_session_id: session_id,
            };
            let outcome = receive_to_file(&listen, &output, &config).await?;
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

fn resolve_addr(addr: &str) -> Result<std::net::SocketAddr, std::io::Error> {
    addr.to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid address"))
}

/// Map legacy flags (`--addr`, `-o` / `--output`) to positional args for clap.
fn normalize_cli_args(mut args: Vec<String>) -> Vec<String> {
    let Some(subcommand) = args.get(1).map(String::as_str) else {
        return args;
    };

    match subcommand {
        "send" => hoist_flag_value(&mut args, "--addr", 3),
        "recv" => {
            hoist_flag_value(&mut args, "--output", 2);
            hoist_flag_value(&mut args, "-o", 2);
        }
        _ => {}
    }

    args
}

/// If `flag` is present, remove it and insert its value at `insert_at`.
fn hoist_flag_value(args: &mut Vec<String>, flag: &str, insert_at: usize) {
    let Some(flag_idx) = args.iter().position(|arg| arg == flag) else {
        return;
    };
    if flag_idx + 1 >= args.len() {
        return;
    }
    let value = args.remove(flag_idx + 1);
    args.remove(flag_idx);
    let insert_at = insert_at.min(args.len());
    args.insert(insert_at, value);
}

#[cfg(test)]
mod tests {
    use super::normalize_cli_args;

    fn s(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn send_positional_addr() {
        let args = normalize_cli_args(s(&[
            "fountain",
            "send",
            "file.bin",
            "127.0.0.1:7878",
        ]));
        assert_eq!(
            args,
            s(&["fountain", "send", "file.bin", "127.0.0.1:7878"])
        );
    }

    #[test]
    fn send_legacy_addr_flag() {
        let args = normalize_cli_args(s(&[
            "fountain",
            "send",
            "file.bin",
            "--addr",
            "127.0.0.1:7878",
        ]));
        assert_eq!(
            args,
            s(&["fountain", "send", "file.bin", "127.0.0.1:7878"])
        );
    }

    #[test]
    fn recv_positional_output() {
        let args = normalize_cli_args(s(&["fountain", "recv", "out.bin"]));
        assert_eq!(args, s(&["fountain", "recv", "out.bin"]));
    }

    #[test]
    fn recv_legacy_output_flags() {
        for flag in ["-o", "--output"] {
            let args = normalize_cli_args(s(&["fountain", "recv", flag, "out.bin"]));
            assert_eq!(args, s(&["fountain", "recv", "out.bin"]), "flag {flag}");
        }
    }
}
