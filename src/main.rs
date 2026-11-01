//! Flotilla: a fleet of Horizon bots in one headless game process.
//!
//! `flotilla up` starts the first bot's headless game and spawns the others into it; `add` and
//! `remove` change the fleet while it runs; `ctl NAME -- ARGS` drives one bot with horizon-ctl.
//! See README.md.

mod config;
mod fleet;
mod fork;

use clap::{Parser, Subcommand};
use config::{Bot, Config};
use fleet::Fleet;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "A fleet of Horizon bots in one headless game process"
)]
struct Cli {
    /// The fleet's file
    #[arg(long, short, default_value = "flotilla.toml", global = true)]
    file: PathBuf,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start the fleet: the first bot's game, then every other bot spawned into it
    Up,
    /// Spawn one more bot into the running fleet (a [[bot]] of the file, or a new one)
    Add {
        name: String,
        #[arg(long)]
        render_distance: Option<u32>,
        /// The launcher's session file of the account it plays as
        #[arg(long)]
        session_file: Option<PathBuf>,
    },
    /// Remove a spawned bot from the running fleet
    Remove { name: String },
    /// The bots playing now and their control sockets
    List,
    /// Run a horizon-ctl command on one bot: `flotilla ctl Scout -- state`
    Ctl {
        name: String,
        #[arg(last = true, required = true)]
        args: Vec<String>,
    },
    /// Stop the fleet
    Down,
    /// Merge upstream's main into the fork's main (fork-main checked out in REPO) and push it
    SyncFork { repo: PathBuf },
    /// Refuse a branch that carries a FORK ONLY commit, before opening a pull request from it
    PrCheck { repo: PathBuf, branch: String },
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("flotilla: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Cmd::SyncFork { repo } => return fork::sync(&repo),
        Cmd::PrCheck { repo, branch } => return fork::pr_check(&repo, &branch),
        _ => {}
    }
    let config = Config::load(&cli.file)?;
    let base = cli
        .file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let base = std::path::absolute(base).map_err(|e| e.to_string())?;
    let paths = config.paths(&base)?;
    let fleet = Fleet { config, paths };
    match cli.command {
        Cmd::Up => fleet.up(),
        Cmd::Add {
            name,
            render_distance,
            session_file,
        } => {
            if !config::valid_name(&name) {
                return Err(format!("{name:?} is not a player name"));
            }
            // A bot of the file keeps its settings unless the command gives others.
            let known = fleet
                .config
                .bots
                .iter()
                .find(|b| b.name.eq_ignore_ascii_case(&name));
            let bot = Bot {
                name: known.map_or(name.clone(), |b| b.name.clone()),
                render_distance: render_distance.or(known.and_then(|b| b.render_distance)),
                session_file: session_file.or(known.and_then(|b| b.session_file.clone())),
            };
            fleet.add(&bot)?;
            fleet.wait_ready(&bot.name)?;
            println!("{} is in the world", bot.name);
            Ok(())
        }
        Cmd::Remove { name } => fleet.remove(&name),
        Cmd::List => {
            for (name, socket) in fleet.list()? {
                println!("{name:<16} {}", socket.display());
            }
            Ok(())
        }
        Cmd::Ctl { name, args } => {
            let status = std::process::Command::new(&fleet.paths.ctl)
                .arg("--socket")
                .arg(fleet.socket(&name))
                .args(&args)
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("horizon-ctl exited with {status}"))
            }
        }
        Cmd::Down => fleet.down(),
        Cmd::SyncFork { .. } | Cmd::PrCheck { .. } => unreachable!("handled above"),
    }
}
