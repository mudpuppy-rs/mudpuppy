[![GitHub branch status](https://img.shields.io/github/actions/workflow/status/mudpuppy-rs/mudpuppy/rust.yml?style=for-the-badge)](https://github.com/mudpuppy-rs/mudpuppy/actions/workflows/rust.yml)
[![GitHub Release](https://img.shields.io/github/v/release/mudpuppy-rs/mudpuppy?include_prereleases&style=for-the-badge)](https://github.com/mudpuppy-rs/mudpuppy/releases)
[![GitHub License](https://img.shields.io/github/license/mudpuppy-rs/mudpuppy?style=for-the-badge&label=License)](https://github.com/mudpuppy-rs/mudpuppy?tab=MIT-1-ov-file#readme)
[![Discord](https://img.shields.io/discord/1292557013276168203?style=for-the-badge&label=Discord)](https://discord.gg/bRadchaGFq)

# Mudpuppy

A terminal [MUD] client with a customizable interface and Python scripting.

> The mudpuppy (Necturus maculosus) is the only completely aquatic salamander in
> Canada. It is also the largest salamander species in the country.[^1]

[^1]: https://ontarionature.org/programs/community-science/reptile-amphibian-atlas/mudpuppy/

[MUD]: https://en.wikipedia.org/wiki/Multi-user_dungeon

# Status

> [!WARNING]
> Mudpuppy is presently an **unfinished prototype**. Do not attempt to use unless
> you are prepared to fix your own issues.
>
> Documentation is sparse and there is **NO** stability guarantee. Everything is
> subject to change without notice.
>
> Mudpuppy should work for a variety of MUD/MUSH/MUX/MUCK games, but it has
> primarily been tested with LP-style MUDs.
>
> Here be dragons^H^H^H^H^H^H^Hrabid saber-toothed mudpuppys.

https://github.com/user-attachments/assets/d89ade8d-7a36-4f14-8e76-17598aaac9a2

# Features

* **Responsive TUI** - Mudpuppy is a terminal client, but it tries to be more
  like a GUI app. It has a terminal user-interface (TUI) with built-in support
  for panes and resizable sub-windows. The TUI scales and redraws as your
  terminal changes size, making it great for use on mobile over SSH.

* **Python** - the "py" in "Mudpup**py**" stands for Python :) Instead of Lua,
  or a custom scripting language, Mudpuppy uses Python for customization and
  extension.

* **Async** - triggers, aliases, and other core client functions are
  asynchronous. Your scripts can benefit from the Python `asyncio` ecosystem,
  making it easy to take complex actions like sending HTTP requests without
  blocking the client.

* **Multi-Session** - Mudpuppy will let you connect to multiple MUDs with one
  client instance. Your triggers/aliases/etc can be global, or specific to
  a single MUD. It's easy to send commands from one session to another.

* **Multi-platform** - Mudpuppy runs well on Linux, MacOS, and Windows (with or
  without WSL).

* **Small** - Mudpuppy is written in Rust and is less than 8mb in size. It has
  no special runtime dependencies other than Python.

# Quick Start

1. Build the client from source.

Binary releases will be provided in the future, but for now you will need to
be comfortable installing Rust and building Mudpuppy from source.

2. Create a config file with details for your favourite MUD. The location of the
   file will differ based on your OS.

| OS      | Config file                                                      |
|---------|------------------------------------------------------------------|
| Linux   | `$HOME/.config/mudpuppy/config.toml`                             |
| MacOS   | `/Users/$USERNAME/Library/Application Support/mudpuppy/config.toml` |
| Windows | `C:\Users\$USER\AppData\Roaming\mudpuppy\config.toml`            |

You can also customize the location with the `MUDPUPPY_CONFIG` and
`MUDPUPPY_DATA` environment variables.

```toml
[[muds]]
name = "Dune (TLS)"
host = "dunemud.net"
port = 6788
tls = "Enabled"

[[muds]]
name = "Dune (Telnet)"
host = "dunemud.net"
port = 6789
tls = "Disabled"
```

3. Run Mudpuppy from a terminal, and get to hacking'n'slashing:

```bash
mudpuppy
```

# Documentation

## User guide

Mudpuppy offers a [user guide] book that provides detailed information and
examples. It's the best place to get started.

[user guide]: https://mudpuppy-rs.github.io/mudpuppy/user-guide/

## API docs

For scripting purposes you might be interested in seeing [API documentation]
for the `mudpuppy_core` module as well as the other Python interfaces.

[API documentation]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/

# Scripting

Python scripts placed in the Mudpuppy config directory are automatically loaded
when Mudpuppy is started. This is the principle mechanism of scripting: putting
Python code that interacts with Mudpuppy through the `mudpuppy` and
`mudpuppy_core` packages in your config dir.

Mudpuppy supports:

* Aliases, matching on user input.
* Triggers, matching on game output.
* Timers, running on fixed intervals.
* Commands, invoked with a special prefix.

Helpful Python decorators and the async nature of Mudpuppy make creating complex
behaviours easy:

```python
import logging
import asyncio
from mudpuppy import alias
from mudpuppy_core import mudpuppy_core

@alias(mud_name="Dune", pattern="^kill (.*)$")
async def kill_headbutt(session_id: int, _alias_id: int, line: str, groups):
    # Send through the original line so that we actually start combat in-game
    # with the 'kill' command.
    await mudpuppy_core.send_line(session_id, line)

    # Wait for a little bit, and then give them a headbutt!
    target = groups[0]
    logging.info(f"building up momentum for a headbutt attack on {target}")

    await asyncio.sleep(5)
    await mudpuppy_core.send_line(session_id, f"headbutt {target}")
```

See the _work-in-progress_ [user guide] and [API documentation] for more
information.

# Development

Mudpuppy's development environment is distributed as a [Nix flake] that can be
activated with `nix develop` in the project directory. This will setup the
required Rust and Python dependencies as well as helpful development tools like
pre-commit hooks for formatting/linting.

Using the flake isn't mandatory to contribute to, or build Mudpuppy, but highly
recommended. If you are not using the Nix Flake you will need Rust 1.74+ and
Python 3.12+ (_techincally Python 3.7+ may be compatible, but 3.12 is the most
tested_).

[Nix flake]: https://zero-to-nix.com/concepts/flakes

## Building

```bash
cargo build           # debug - compiles faster, runs much slower!
cargo build --release # release - compiles slowwww, runs very fast!
```

## Priorities

Rough development priorities:

* Documentation.
* Persistence for `/trigger`, `/alias`, `/timer`.
* API/config stability.
* Test coverage.
* Styling/themes.
* Multi-line input area support.
* ????

# Alternatives

Mudpuppy is inspired by many other great MUD clients. You may wish to try one of
these if Mudpuppy doesn't strike your fancy.

* **[Blightmud]** - another terminal MUD client written in Rust. You may prefer
  this client if you like Lua for scripting.

* **[TinTin++]** - a well established terminal MUD client with a custom
  scripting language. You may prefer this client if you value stability and
  want a variety of features.

* **[Mudlet]** - the best open-source GUI-based MUD client around. You may
  prefer this client if you want to avoid the terminal in favour of a graphical
  UI.

[Blightmud]: https://github.com/blightmud/blightmud
[TinTin++]: https://tintin.mudhalla.net/
[Mudlet]: https://www.mudlet.org/
