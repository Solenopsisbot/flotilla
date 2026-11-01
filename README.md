# Flotilla

A fleet of [Horizon](https://github.com/horizon-rust/horizon) bots in **one headless game process**.

The first bot starts a headless Horizon (`--headless`: no display, window or GPU). Every other bot is
`spawn`ed into that same process, so they all share the game's tables (block states, models, recipes:
about 370 MB) instead of each loading its own. Bots can join and leave while the fleet runs.

Measured on a Ryzen 7 3700X with every bot joined to a local server:

| Bots | One game each | One Flotilla process |
|---|---|---|
| 1 | ~400 MB | 396 MB, 1% of a core |
| 10 | ~4 GB | 772 MB, 6% of a core |
| 50 | ~20 GB | 2.47 GB, 43% of a core (1.94 GB, 37% at render distance 3) |

Each bot is driven through its own `horizon-ctl` control socket: walk, look, mine, place, use, chat, read
the world. Sextant (pathing, coming next) builds on that.

## What it needs

- **A Horizon build with `--headless` and `spawn`.** Until those land upstream (horizon-rust/horizon#4951,
  #4952), that's the `main` branch of [Solenopsisbot/horizon](https://github.com/Solenopsisbot/horizon):
  ```sh
  git clone -b main https://github.com/Solenopsisbot/horizon.git horizon-fork
  cd horizon-fork
  cargo run --release -p horizon-prepare -- --prepare --server-jar ~/Downloads/server.jar   # once, from your own Minecraft 26.3
  cargo build --profile fast -p horizon-client
  ```
- **Accounts.** Upstream Horizon follows Mojang's rules: every bot plays as a signed-in account that owns
  Minecraft: Java Edition, given as a session file from the Horizon launcher (`session_file`). Only the
  fork's `main` also lets a bot without one play **offline** under its name, for your own offline-mode
  servers.

## Use

`flotilla.toml`:

```toml
server = "127.0.0.1:25565"
horizon = "~/horizon-fork"   # the checkout above (binaries from target/fast, game files from .local)
render_distance = 3          # chunks each bot asks for: most of a bot's memory

[[bot]]
name = "Scout"               # the first bot is the fleet's process

[[bot]]
name = "Miner"
render_distance = 4
# session_file = "/path/to/session.json"   # a signed-in account
```

```sh
flotilla up                        # start the fleet: Scout's game, then every other bot spawned into it
flotilla list                      # the bots playing now and their control sockets
flotilla ctl Miner -- state        # any horizon-ctl command, on one bot
flotilla ctl Miner -- goto 10 64 -20
flotilla add Courier --render-distance 2   # one more bot, while the fleet runs
flotilla remove Miner              # a spawned bot leaves
flotilla down                      # stop the fleet (the first bot's process, and every bot in it)
```

The fleet keeps its sockets and its game folder in `.flotilla/` beside `flotilla.toml` (`state = ...`
changes it).

## The fork's rules

The fork's `main` is upstream's `main`, plus our in-flight branches, plus commits whose subject starts with
`FORK ONLY` (offline bots). Two commands keep that safe:

```sh
flotilla sync-fork ~/horizon-fork          # merge upstream's main into fork-main (a worktree of it) and push it, fast-forward only
flotilla pr-check ~/horizon some-branch    # refuse a branch that carries a FORK ONLY commit, before a pull request
```

Never `gh repo sync --force` the fork: it would drop the fork-only commits. Pull requests to upstream come
from branches on upstream's `main`, never from the fork's `main`.

## Building

```sh
cargo build --release   # target/release/flotilla
cargo test
```
