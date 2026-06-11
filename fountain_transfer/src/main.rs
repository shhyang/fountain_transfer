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
        path: PathBuf,
        #[arg(long, value_name = "HOST:PORT")]
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
        #[arg(long, default_value_t = 2)]
        delay_ms: u64,
    },
    /// Listen for symbols and write the recovered object.
    Recv {
        #[arg(long, value_name = "HOST:PORT", default_value = "0.0.0.0:7878")]
        listen: String,
        #[arg(short, long, value_name = "PATH")]
        output: PathBuf,
        #[arg(long)]
        session_id: Option<u64>,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
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
            listen,
            output,
            session_id,
            timeout_secs,
        } => {
            let config = RecvConfig {
                recv_timeout: Duration::from_secs(timeout_secs),
                expected_session_id: session_id,
            };
            let outcome = receive_to_file(&listen, &output, &config).await?;
            eprintln!(
                "received {} bytes from {} ({} packets)",
                outcome.object.len(),
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
