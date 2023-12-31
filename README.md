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

# Features

* **Responsive TUI** - Mudpuppy is a terminal client, but it tries to be more
  like a GUI app. It has a terminal user-interface (TUI) with built-in support
  for panes, resizable sub-windows, and so on. The TUI scales and redraws
  as your terminal changes size, making it great for use on mobile over SSH.

* **Python** - the "py" in "Mudpup**py**" stands for Python :) Instead of Lua,
  or a custom scripting language, Mudpuppy uses Python for customization and
  extension.

* **Async** - triggers, aliases, and other core client functions are
  asynchronous. Your scripts can benefit from the Python `asyncio` ecosystem,
  making it easy to take complex actions like making HTTP requests without
  blocking the client.

* **Multi-Session** - Mudpuppy will let you connect to multiple MUDs with one
  client instance. Your triggers/aliases/etc can be global, or specific to
  a single MUD. It's easy to send commands from one session to another.

* **Multi-platform** - Mudpuppy runs well on Linux, MacOS, and Windows (with or
  without WSL).

* **Small** - Mudpuppy is written in Rust and is less than 8mb in size. It has
  no special runtime dependencies other than Python.

# Quick Start

1. Download a release:

```bash
# TODO...
```

Advanced users may want to build from source. TODO: link to instructions.

2. Create a config file with details for your favourite MUD. The location of the
   file will differ based on your OS.

| OS      | Config file                                                      |
|---------|------------------------------------------------------------------|
| Linux   | `$HOME/.config/mudpuppy/config.toml`                             |
| MacOS   | `/Users/$USERNAME/Library/Application Support/mudpuppy/config.toml` |
| Windows | `C:\Users\$USER\AppData\Roaming\mudpuppy\config.toml`            |

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

See this example config file for more information. TODO: link to config file.

3. Run Mudpuppy from a terminal, and get to hacking'n'slashing:

```bash
mudpuppy
```

# Scripting

Python scripts placed in the Mudpuppy config directory are automatically loaded
when Mudpuppy is started. This is the principle mechanism of scripting: putting
Python code that interacts with Mudpuppy through the `mudpuppy` and
`mudpuppy_core` packages in your config dir.

Since Mudpuppy supports connecting to multiple MUDs at one time, you will
typically need to decide whether your scripts are targeting one specific MUD,
or all MUDs.

Remember that Mudpuppy is async: most callbacks will need to be async, and you
will need to await most operations on the `mudpuppy_core` interface.

Full documentation of the various APIs and scripting patterns is pending. For
now here's a run-down of the core bits. See the examples in this repo for more
detail. TODO: link examples.

## Creating an Alias

Aliases map a command pattern to a specific action. They can target a specific
MUD or all MUDs.

The line that matched the alias, as well as any regexp groups in the pattern are
provided to the alias callback function alongside the `SessionId` of the MUD.

- **Target a specific MUD**: Use `mud_name` to create an alias for a specific MUD:
    ```python
    @alias(mud_name="Dune (TLS)", pattern="^test$", name="Test Alias")
    async def test(session_id: SessionId, _alias_id: AliasId, _line: str, _groups):
        # Send a command when the alias is run
        mudpuppy_core.send(session_id, "say this is an alias test!")
    ```

- **Simple commands**: Use 'expansion` to simplify the above example:
    ```python
    @alias(mud_name="Dune (TLS)", pattern="^test$", name="Test Alias", expansion='say this is an alias test!')
    async def test(_session_id: SessionId, _alias_id: AliasId, _line: str, _groups):
        # Since we simply want to send an expansion we don't need to do anything
        # in the function body anymore!
        pass
    ```

- **Global Alias**: Create an alias that works across all MUDs by omitting `mud_name`:
    ```python
    @alias(pattern="^e$", name="Quick East", expansion="east")
    async def quick_east(_session_id: SessionId, _alias_id: AliasId, _line: str, _groups):
        pass
    ```

## Creating a Trigger

Triggers execute based on matching text patterns in the MUD output. They can also
be scoped to specific MUDs or be global.

The line that matched the trigger, as well as any regexp groups in the pattern
are provided to the trigger callback function alongside the `SessionId` of the MUD
and the `TriggerId` of the trigger. Multi-line triggers are not yet supported.

Like aliases you can make these global by omitting `mud_name`.

- **Specific MUD Trigger**:
    ```python
    @trigger(
        mud_name="Dune (TLS)",
        pattern="^You (say|exclaim|ask): (.*) narf.$",
        expansion="say N A R F!!!",
    )
    async def narf(_session_id: SessionId, _trigger_id: TriggerId, _line: str, _groups):
        pass
    ```

#### Creating a Prompt Handler

Prompt handlers are invoked when Mudpuppy receives a prompt from the MUD. Like
aliases and triggers these can target all MUDs or be bound to a particular MUD.
How Mudpuppy determines what is/isn't a prompt is somewhat complex and described
separately. TODO: Link to prompt stuff.

The prompt event is provided as an argument to the handler and can be used to
find the text of the prompt and the `SessionId` of the MUD.

- **Prompt Handler for a Specific MUD**:
    ```python
    @on_mud_event("Dune (TLS)", EventType.Prompt)
    async def test_prompt_handler(event: Event):
        logging.debug(f'test prompt is: "{str(event.prompt)}"')
        if str(event.prompt) == "Please enter your name: ":
            await mudpuppy_core.send_line(event.id, "cooldude1999")
    ```

- **Global Prompt Handler**:
    ```python
    @on_event(EventType.Prompt)
    async def prompt_handler(event: Event):
        logging.debug(f"prompt event: {event}")
    ```

#### Creating a Timer

Timers allow you to execute actions periodically, either globally or for
a specific MUD.

The timer function receives the `TimerId` of the timer, and potentially the
`SessionId` of the MUD. This is optional since global timers are run without
being tied to a specific session.

- **Global Timer**:
    ```python
    # A timer that runs every 2 minutes, 10 seconds
    @timer(name="Global Test Timer", seconds=10, minutes=2)
    async def global_woohoo(timer_id: TimerId, _session_id: Optional[SessionId]):
        logging.debug(f"2m10s timer fired: {timer_id}!")
    ```

- **Specific MUD Timer**:
    ```python
    # A timer that runs every 5 seconds.
    @timer(mud_name="Dune (TLS)", name="Dune MUD Timer", seconds=5)
    async def test_timer(timer_id: TimerId, session_id: Optional[SessionId]):
        assert session_id is not None
        await mudpuppy_core.add_output(
            session_id, OutputItem.command_result(f"[{timer_id}] Tick tock!")
        )
        await mudpuppy_core.send_line(session_id, "say tick tock")
    ```

- **Max Ticks**:
    ```python
    # A timer that runs every 10 minutes, but only 2 times.
    @timer(name="Global Test Timer", minutes=10, max_ticks=2)
    async def global_woohoo(timer_id: TimerId, _session_id: Optional[SessionId]):
        logging.debug(f"global timer fired: {timer_id}!")
    ```

# Client Commands

In addition to scripting Mudpuppy offers some built-in commands, identified with
a `/` prefix. The choice of prefix can be changed in your config file.

## `/status`

Shows the current connection status. Use `/status --verbose` for more
information.

## `/connect`

Connects the current session if it isn't already connected.

## `/disconnect`

Disconnects the current session if it isn't already disconnected.

## `/quit`

Exits Mudpuppy.

## `/reload`

Reloads user python scripts. Scripts can define a handler to be called before
reloading occurs if any clean-up needs to be done:

```python
# Called before /reload completes and the module is re-imported.
def __reload__():
    pass
```

## `/alias`, `/trigger`, `/timer`

These commands allow creating simple aliases/triggers/timers that last only for
the duration of the session. To create durable versions pref Python scripting.

# Prompt Detection

Mudpuppy attempts to detect what is/isn't a prompt in a few ways (listed in
order of how reliable they are):

* Negotiating support for the telnet "EOR" option, and expecting prompts to be
  terminated with EOR.
* Seeing lines that end with telnet "GA", and assuming they are prompts.
* Seeing lines that end without `\r\n`, after a short timeout expires to ensure
  it wasn't a partial line.

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
