//! Running a fleet: one headless Horizon process whose first player is the first bot, and
//! every other bot spawned into it (`horizon-ctl spawn`), so they share the game's tables:
//! about 30-40 MB a bot instead of about 400 MB for a game of its own.
//!
//! Everything goes through `horizon-ctl`, Horizon's own control client: Flotilla never speaks
//! the control socket's protocol itself, so it keeps working when that protocol changes.

use crate::config::{Bot, Config, Paths};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// How long `up` waits for each bot to stand in the world.
const READY_SECONDS: &str = "120";

pub struct Fleet {
    pub config: Config,
    pub paths: Paths,
}

impl Fleet {
    /// `horizon-ctl --socket SOCKET ARGS…`, its JSON reply, or why it failed.
    fn ctl(&self, socket: &Path, args: &[&str]) -> Result<Value, String> {
        let out = Command::new(&self.paths.ctl)
            .arg("--socket")
            .arg(socket)
            .args(args)
            // The game folder: `launch` starts the game here, and relative paths resolve here.
            .current_dir(self.paths.game_dir())
            .env("HORIZON_HOME", self.paths.game_dir())
            .output()
            .map_err(|e| format!("{}: {e}", self.paths.ctl.display()))?;
        let text = String::from_utf8_lossy(&out.stdout);
        // horizon-ctl prints one JSON reply; an error reply has an "error" field.
        let reply: Value = serde_json::from_str(text.trim()).map_err(|_| {
            let err = String::from_utf8_lossy(&out.stderr);
            format!(
                "horizon-ctl {}: {}{}",
                args.join(" "),
                text.trim(),
                err.trim()
            )
        })?;
        match reply.get("error").and_then(Value::as_str) {
            Some(e) => Err(format!("horizon-ctl {}: {e}", args.first().unwrap_or(&""))),
            None => Ok(reply),
        }
    }

    /// The first bot's process answers: the fleet is up.
    pub fn is_up(&self) -> bool {
        self.paths.host_socket().exists()
            && self.ctl(&self.paths.host_socket(), &["status"]).is_ok()
    }

    /// Starts the fleet: the first bot's game, then every other bot spawned into it.
    pub fn up(&self) -> Result<(), String> {
        if self.is_up() {
            return Err("the fleet is already up (flotilla list, flotilla down)".into());
        }
        std::fs::create_dir_all(self.paths.game_dir()).map_err(|e| e.to_string())?;
        let (first, rest) = self.config.bots.split_first().ok_or("no bots")?;
        let mut args: Vec<String> = vec![
            "launch".into(),
            "--game".into(),
            self.paths.client.display().to_string(),
            "--resources".into(),
            self.paths.resources.display().to_string(),
            "--headless".into(),
            "--server".into(),
            self.config.server.clone(),
            "--name".into(),
            first.name.clone(),
        ];
        if let Some(d) = self.config.render_distance(first) {
            args.extend(["--render-distance".into(), d.to_string()]);
        }
        if let Some(f) = &first.session_file {
            args.extend(["--session-file".into(), f.display().to_string()]);
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.ctl(&self.paths.host_socket(), &args)?;
        self.ctl(&self.paths.host_socket(), &["wait-ready", READY_SECONDS])?;
        println!("{} is up (the fleet's process)", first.name);
        for bot in rest {
            self.add(bot)?;
        }
        for bot in rest {
            self.wait_ready(&bot.name)?;
            println!("{} is in the world", bot.name);
        }
        Ok(())
    }

    /// Waits until spawned bot `name` stands in the world. `spawn` answers as soon as the
    /// bot's thread starts, a moment before its control socket exists: wait for the socket
    /// first, or `wait-ready` finds nothing to connect to.
    pub fn wait_ready(&self, name: &str) -> Result<(), String> {
        let socket = self.paths.bot_socket(name);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !socket.exists() {
            if std::time::Instant::now() > deadline {
                return Err(format!(
                    "{name}'s control socket never appeared: see the game's log in {}",
                    self.paths.game_dir().display()
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        self.ctl(&socket, &["wait-ready", READY_SECONDS])
            .map(|_| ())
    }

    /// Spawns one more bot into the running fleet (it is still joining when this returns).
    pub fn add(&self, bot: &Bot) -> Result<(), String> {
        let mut args = vec!["spawn".to_string(), bot.name.clone()];
        if let Some(d) = self.config.render_distance(bot) {
            args.extend(["--render-distance".into(), d.to_string()]);
        }
        if let Some(f) = &bot.session_file {
            args.extend(["--session-file".into(), f.display().to_string()]);
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.ctl(&self.paths.host_socket(), &args)?;
        println!("{} spawned", bot.name);
        Ok(())
    }

    /// Removes a spawned bot. The first bot is the process itself: `down` stops it.
    pub fn remove(&self, name: &str) -> Result<(), String> {
        if self
            .config
            .bots
            .first()
            .is_some_and(|b| b.name.eq_ignore_ascii_case(name))
        {
            return Err(format!(
                "{name} is the fleet's process: `flotilla down` stops the fleet"
            ));
        }
        self.ctl(&self.paths.bot_socket(name), &["quit"])?;
        println!("{name} left");
        Ok(())
    }

    /// The bots playing now and their control sockets.
    pub fn list(&self) -> Result<Vec<(String, std::path::PathBuf)>, String> {
        let first = self.config.bots.first().ok_or("no bots")?;
        let mut bots = vec![(first.name.clone(), self.paths.host_socket())];
        let reply = self.ctl(&self.paths.host_socket(), &["players"])?;
        for name in reply["spawned"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            bots.push((name.to_owned(), self.paths.bot_socket(name)));
        }
        Ok(bots)
    }

    /// Stops the fleet: the first bot quits, which ends the process and every bot in it.
    pub fn down(&self) -> Result<(), String> {
        self.ctl(&self.paths.host_socket(), &["quit"])?;
        println!("the fleet is down");
        Ok(())
    }

    /// The socket of bot `name`, to drive it with horizon-ctl.
    pub fn socket(&self, name: &str) -> std::path::PathBuf {
        match self.config.bots.first() {
            Some(first) if first.name.eq_ignore_ascii_case(name) => self.paths.host_socket(),
            _ => self.paths.bot_socket(name),
        }
    }
}
