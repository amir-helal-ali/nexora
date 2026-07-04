//! NXP CLI — `nxp` command-line tool.
//!
//! Implements a subset of the commands specified in Nexora Part 12:
//! - `nxp ping <addr>` — connect to an NXP server and send a PING frame
//! - `nxp sniff <file>` — read frames from a file and pretty-print them
//! - `nxp version` — print the protocol version
//! - `nxp keygen` — generate a fresh Ed25519 identity keypair

use anyhow::Result;
use clap::{Parser, Subcommand};
use nxp_core::{Frame, FrameFlags, FrameHeader, Opcode, VERSION, NONCE_LEN};
use nxp_security::IdentityKey;
use nxp_session::time::now_us;
use nxp_transport::NxpClient;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "nxp",
    version,
    about = "NXP — Nexora Exchange Protocol CLI"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Print the NXP wire-format version.
    Version,
    /// Generate a fresh Ed25519 identity keypair.
    Keygen,
    /// Connect to an NXP server and send a PING frame.
    Ping {
        /// Server address, e.g. `127.0.0.1:4433`.
        addr: SocketAddr,
        /// Server name (for SNI / cert).
        #[arg(long, default_value = "nexora.internal")]
        name: String,
    },
    /// Read NXP frames from a binary file and pretty-print them.
    Sniff {
        /// Path to the file containing raw NXP frames.
        file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Version => {
            println!("NXP wire-format version: {}", VERSION);
            println!("Magic: 0x{:02X}{:02X} ('{}{}')",
                nxp_core::MAGIC[0], nxp_core::MAGIC[1],
                nxp_core::MAGIC[0] as char, nxp_core::MAGIC[1] as char,
            );
            println!("Header length: {} bytes", nxp_core::HEADER_LEN);
            println!("Auth tag length: {} bytes", nxp_core::AUTH_TAG_LEN);
            println!("Signature length: {} bytes", nxp_core::SIGNATURE_LEN);
        }
        Cmd::Keygen => {
            let key = IdentityKey::generate();
            println!("Ed25519 identity keypair");
            println!("  public  (32B): {}", hex::encode(key.public_bytes()));
            // For demonstration, we do NOT print the private key to stdout.
            // In production this would be written to a vault / KMS.
            println!("  (private key withheld from stdout; see vault integration)");
        }
        Cmd::Ping { addr, name } => {
            ping(addr, &name).await?;
        }
        Cmd::Sniff { file } => {
            sniff(&file)?;
        }
    }
    Ok(())
}

async fn ping(addr: SocketAddr, name: &str) -> Result<()> {
    let client = NxpClient::new()?;
    eprintln!("connecting to {} ({})...", addr, name);
    let mut conn = client.connect(addr, name).await?;
    eprintln!("connected; sending PING frame");

    let header = FrameHeader {
        version: VERSION,
        flags: FrameFlags::NONE, // unencrypted PING for the MVP demo
        opcode: Opcode::Ping,
        stream_id: 1,
        request_id: 1,
        timestamp_us: now_us(),
        nonce: [0u8; NONCE_LEN],
        payload_len: 0,
    };
    let frame = Frame {
        header,
        payload: bytes::Bytes::new(),
        auth_tag: [0u8; nxp_core::AUTH_TAG_LEN],
        signature: None,
    };
    conn.send_frame(&frame).await?;
    eprintln!("PING sent; awaiting response");

    let resp = conn.recv_frame().await?;
    println!("Response:");
    println!("  opcode:     {:?}", resp.header.opcode);
    println!("  stream_id:  {}", resp.header.stream_id);
    println!("  request_id: {}", resp.header.request_id);
    println!("  flags:      {:?}", resp.header.flags);
    println!("  payload_len: {}", resp.payload.len());
    Ok(())
}

fn sniff(file: &PathBuf) -> Result<()> {
    let data = std::fs::read(file)?;
    let mut offset = 0usize;
    let mut frame_no = 0usize;
    while offset < data.len() {
        match Frame::peek_required_len(&data[offset..])? {
            None => break,
            Some(needed) => {
                if offset + needed > data.len() {
                    eprintln!("trailing {} bytes (incomplete frame)", data.len() - offset);
                    break;
                }
                let frame = Frame::decode(&data[offset..offset + needed])?;
                println!(
                    "[{:04}] opcode={:?} stream={} req={} flags={:?} payload={}B",
                    frame_no,
                    frame.header.opcode,
                    frame.header.stream_id,
                    frame.header.request_id,
                    frame.header.flags,
                    frame.payload.len(),
                );
                offset += needed;
                frame_no += 1;
            }
        }
    }
    println!("--- {} frame(s) ---", frame_no);
    Ok(())
}
