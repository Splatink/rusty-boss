use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use rusty_boss::{
    document::{Backup, LiveSet, Patch},
    protocol::{Address, Message},
    schema::Schema,
};
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    name = "rusty-boss",
    about = "Rusty Boss — independent Linux Katana Gen 3 editor (experimental)",
    version
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Open the native desktop editor (also the default)
    Gui,
    /// List recovered blocks and parameter counts; no hardware access
    Schema {
        #[arg(long)]
        filter: Option<String>,
    },
    /// Validate and summarize a Gen 3 TSL or ALB file
    Inspect { path: PathBuf },
    /// Make a factory-default offline TSL (not an amplifier backup)
    New {
        path: PathBuf,
        #[arg(long, default_value = "Rusty Boss")]
        name: String,
    },
    /// Print a checked RQ1 packet without sending it; address is packed hex
    Request { address: String, size: u32 },
    /// Strict read-only reverb diagnostic; no editor-mode setup or any DT1 writes
    ReadReverb {
        /// Explicit ALSA raw MIDI port from `ports`
        #[arg(long)]
        port: String,
    },
    /// List duplex ALSA raw MIDI ports (never auto-connects)
    Ports,
}
fn main() -> Result<()> {
    let args = Args::parse();
    match args.command.unwrap_or(Command::Gui) {
        Command::Gui => {
            #[cfg(feature = "gui")]
            {
                rusty_boss::gui::run().map_err(|e| anyhow::anyhow!(e.to_string()))?;
            }
            #[cfg(not(feature = "gui"))]
            {
                bail!("build with --features gui for the desktop UI");
            }
        }
        Command::Schema { filter } => {
            let schema = Schema::load();
            for b in &schema.blocks {
                if filter
                    .as_ref()
                    .is_none_or(|f| b.id.to_lowercase().contains(&f.to_lowercase()))
                {
                    println!(
                        "{}  {:42} {:4} bytes {:4} parameters{}",
                        b.address(),
                        b.id,
                        b.size,
                        b.parameters.len(),
                        if b.readonly { " [read-only]" } else { "" }
                    );
                }
            }
        }
        Command::Inspect { path } => {
            let schema = Schema::load();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("alb"))
            {
                let b = Backup::load(&path)?;
                println!(
                    "{}: {} system blocks, {} backup rows ({} populated)",
                    b.model,
                    b.system.len(),
                    b.patches.len(),
                    b.patches.iter().flatten().count()
                );
            } else {
                let (set, warnings) = LiveSet::load(&path, &schema)?;
                println!("{} — {} patches", set.name, set.data[0].len());
                for (i, p) in set.data[0].iter().enumerate() {
                    println!("{:3} {}", i + 1, p.name());
                }
                if !warnings.is_empty() {
                    eprintln!(
                        "{} missing/short blocks filled with defaults; not a hardware backup",
                        warnings.len()
                    );
                }
            }
        }
        Command::New { path, name } => {
            if path.exists() {
                bail!("destination already exists; refusing to overwrite");
            }
            let schema = Schema::load();
            let mut p = Patch::new(&schema)?;
            p.rename(&schema, &name)?;
            let mut set = LiveSet::new(&name);
            set.data[0].push(p);
            set.save(&path)?;
            println!("Created {} (offline defaults)", path.display());
        }
        Command::Request { address, size } => {
            let address =
                Address::packed(u32::from_str_radix(address.trim_start_matches("0x"), 16)?)?;
            let bytes = Message::Request {
                device: 16,
                address,
                size,
            }
            .encode()?;
            println!(
                "{}",
                bytes
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
        Command::ReadReverb { port } => {
            #[cfg(feature = "midi")]
            {
                let transport = rusty_boss::midi::AlsaTransport::open(
                    &port,
                    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                )?;
                let mut report =
                    rusty_boss::diagnostics::reverb_snapshot(transport, &Schema::load());
                report["port"] = serde_json::Value::String(port);
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report["error"].is_null() {
                    bail!("Read-only probe incomplete; see the JSON report (no writes attempted)");
                }
            }
            #[cfg(not(feature = "midi"))]
            {
                let _ = port;
                bail!("build with --features midi for ALSA support");
            }
        }
        Command::Ports => {
            #[cfg(feature = "midi")]
            {
                let ports = rusty_boss::midi::ports()?;
                if ports.is_empty() {
                    println!(
                        "No duplex raw MIDI ports. Connect the amp via USB and check ALSA driver support."
                    );
                }
                for p in ports {
                    println!("{}  {}", p.address, p.name);
                }
            }
            #[cfg(not(feature = "midi"))]
            {
                bail!("build with --features midi for ALSA support");
            }
        }
    }
    Ok(())
}
