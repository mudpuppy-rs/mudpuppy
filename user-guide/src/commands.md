# Commands

Mudpuppy has several built-in commands you can run from within the client. By
default the command prefix is "/". The choice of prefix can be changed in your
config file.

## `/status`

Shows the current connection status. Use `/status --verbose` for more
information like the IP address of the MUD and any relevant TLS details.

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

## `/bindings`

View the configured key bindings. You can show only bindings for a specific
input mode by providing `--mode` to the list sub command, e.g.:

```
/bindings list --mode=mudsession
```

See [Key Bindings](config/keybindings.md) for more information.

## `/py`

Allows running Python expressions or statements. If an expression returns an
awaitable, it will be awaited automatically. You are free to `import` other
modules as needed, and define your own functions/variables/etc.

By default several helpful items are provided in-scope:

* `mudpuppy` - the [mudpuppy module].
* `commands` - the [commands module].
* `config` - the result from `mudpuppy_core.config()`.
* `session` - the current session ID.
* `session_info` - the current [SessionInfo].
* `cformat` - the `cformat.cformat()` function.
* `history` - the history module (documentation TBD).

[mudpuppy module]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy.html
[commands module]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/commands.html
[mudpuppy_core.config()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#MudpuppyCore.config
[SessionInfo]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#SessionInfo
[cformat.cformat()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/cformat.html#cformat
