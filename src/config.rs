//! `flotilla.toml`: which Horizon build runs the fleet, which server it joins, and its bots.
//!
//! ```toml
//! server = "127.0.0.1:25565"
//! horizon = "~/horizon-fork"   # a Horizon checkout built with `cargo build --profile fast -p horizon-client`
//! render_distance = 3          # chunks each bot asks the server for (most of a bot's memory)
//!
//! [[bot]]
//! name = "Scout"               # the first bot is the fleet's process; the others are spawned into it
//!
//! [[bot]]
//! name = "Miner"
//! render_distance = 4
//! session_file = "/path/to/session.json"   # a signed-in account; without one a bot plays offline,
//!                                          # which only the Solenopsisbot fork's main allows
//! ```

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The whole file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// `HOST[:PORT]` of the server every bot joins.
    pub server: String,
    /// A Horizon checkout: the binaries come from its `target/fast/` and the prepared game
    /// files from its `.local/horizon-client/26.3`, unless `client`, `ctl` or `resources` say
    /// otherwise.
    pub horizon: Option<PathBuf>,
    pub client: Option<PathBuf>,
    pub ctl: Option<PathBuf>,
    pub resources: Option<PathBuf>,
    /// Where the fleet keeps its control sockets and its game folder (default `.flotilla`
    /// beside this file).
    pub state: Option<PathBuf>,
    /// The render distance of bots that don't set their own (default: the game's).
    pub render_distance: Option<u32>,
    #[serde(default, rename = "bot")]
    pub bots: Vec<Bot>,
}

/// One `[[bot]]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bot {
    pub name: String,
    pub render_distance: Option<u32>,
    /// The launcher's session file of the account this bot plays as.
    pub session_file: Option<PathBuf>,
}

/// The files a fleet uses, resolved from a [`Config`] and the folder its file is in.
#[derive(Debug, Clone)]
pub struct Paths {
    pub client: PathBuf,
    pub ctl: PathBuf,
    pub resources: PathBuf,
    pub state: PathBuf,
}

impl Paths {
    /// The game folder: the first bot's working directory and `HORIZON_HOME`. Spawned bots
    /// share the process, so they share it too.
    pub fn game_dir(&self) -> PathBuf {
        self.state.join("game")
    }
    /// The first bot's control socket.
    pub fn host_socket(&self) -> PathBuf {
        self.state.join("ctl.sock")
    }
    /// A spawned bot's control socket: Horizon puts it beside the first one, with `-NAME`
    /// before the extension (`game/spawn.rs`, `socket_beside`).
    pub fn bot_socket(&self, name: &str) -> PathBuf {
        self.state.join(format!("ctl-{name}.sock"))
    }
}

impl Config {
    pub fn load(file: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let config: Config =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        config.check()?;
        Ok(config)
    }

    /// What a fleet can't start with: no bots, two bots with one name, a name the game
    /// refuses (1 to 16 printable characters, no spaces).
    fn check(&self) -> Result<(), String> {
        if self.bots.is_empty() {
            return Err(
                "flotilla.toml has no [[bot]]: the first one is the fleet's process".into(),
            );
        }
        let mut seen = std::collections::HashSet::new();
        for bot in &self.bots {
            if !valid_name(&bot.name) {
                return Err(format!(
                    "{:?} is not a player name (1 to 16 characters, no spaces)",
                    bot.name
                ));
            }
            if !seen.insert(bot.name.to_ascii_lowercase()) {
                return Err(format!("two bots are called {}", bot.name));
            }
        }
        Ok(())
    }

    /// Paths relative to `base`, the folder `flotilla.toml` is in; `~` is the home folder.
    pub fn paths(&self, base: &Path) -> Result<Paths, String> {
        let at = |p: &Path| {
            let p = expand_home(p);
            if p.is_absolute() { p } else { base.join(p) }
        };
        let horizon = self.horizon.as_deref().map(at);
        let from_horizon = |rel: &str, what: &str, own: &Option<PathBuf>| match (own, &horizon) {
            (Some(p), _) => Ok(at(p)),
            (None, Some(h)) => Ok(h.join(rel)),
            (None, None) => Err(format!(
                "flotilla.toml needs `horizon` (a checkout) or `{what}`"
            )),
        };
        Ok(Paths {
            client: from_horizon("target/fast/horizon-client", "client", &self.client)?,
            ctl: from_horizon("target/fast/horizon-ctl", "ctl", &self.ctl)?,
            resources: from_horizon(".local/horizon-client/26.3", "resources", &self.resources)?,
            state: at(self.state.as_deref().unwrap_or(Path::new(".flotilla"))),
        })
    }

    /// A bot's render distance: its own, else the fleet's.
    pub fn render_distance(&self, bot: &Bot) -> Option<u32> {
        bot.render_distance.or(self.render_distance)
    }
}

/// The game's own rule (`lan::conn::valid_name`): 1 to 16 printable ASCII characters.
pub fn valid_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_graphic())
}

fn expand_home(p: &Path) -> PathBuf {
    match (p.strip_prefix("~"), std::env::var_os("HOME")) {
        (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => p.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config, String> {
        let c: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        c.check().map(|()| c)
    }

    #[test]
    fn a_fleet_reads_its_bots_and_paths() {
        let c = parse(
            r#"
            server = "127.0.0.1:25565"
            horizon = "/src/horizon"
            render_distance = 3
            [[bot]]
            name = "Scout"
            [[bot]]
            name = "Miner"
            render_distance = 5
            "#,
        )
        .unwrap();
        assert_eq!(c.bots.len(), 2);
        assert_eq!(c.render_distance(&c.bots[0]), Some(3));
        assert_eq!(c.render_distance(&c.bots[1]), Some(5));
        let p = c.paths(Path::new("/fleets/one")).unwrap();
        assert_eq!(
            p.client,
            Path::new("/src/horizon/target/fast/horizon-client")
        );
        assert_eq!(
            p.resources,
            Path::new("/src/horizon/.local/horizon-client/26.3")
        );
        assert_eq!(p.state, Path::new("/fleets/one/.flotilla"));
        assert_eq!(
            p.bot_socket("Miner"),
            Path::new("/fleets/one/.flotilla/ctl-Miner.sock")
        );
    }

    #[test]
    fn bad_fleets_are_refused() {
        let base = "server = \"s\"\nhorizon = \"/h\"\n";
        assert!(parse(base).unwrap_err().contains("no [[bot]]"));
        assert!(
            parse(&format!("{base}[[bot]]\nname = \"a b\"\n"))
                .unwrap_err()
                .contains("not a player name")
        );
        let twice = format!("{base}[[bot]]\nname = \"Ann\"\n[[bot]]\nname = \"ann\"\n");
        assert!(parse(&twice).unwrap_err().contains("two bots"));
        assert!(parse(&format!("{base}typo = 1\n[[bot]]\nname = \"A\"\n")).is_err());
    }
}
