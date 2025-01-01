# Configuration

## Where is my config file?

This depends on your OS, but will generally be where applications keep their
configuration:

| OS      | Config dir                                                           |
|---------|----------------------------------------------------------------------|
| Linux   | `$HOME/.config/mudpuppy/config.toml`                                 |
| MacOS   | `/Users/$USERNAME/Library/Application Support/mudpuppy/config.toml`  |
| Windows | `C:\Users\$USER\AppData\Roaming\mudpuppy\config.toml`                |

You can also find this directory from within Mudpuppy by running:

```
/py mudpuppy_core.config_dir()
```

Or from a Python script with:

```python
from mudpuppy_core import mudpuppy_core
path = mudpuppy_core.config_dir()
```

### Customizing config/data directories

You can also set the `MUDPUPPY_CONFIG` and `MUDPUPPY_DATA` environment variables
to customize the config and data dir that Mudpuppy will use. For example, on
a UNIX-like operating system you could run:

```bash
MUDPUPPY_CONFIG=$HOME/mudpuppy-test/config MUDPUPPY_DATA=$HOME/mudpuppy-test/data mudpuppy
```

### Example Config

```toml
[[muds]]
name = "DuneMUD (TLS)"
host = "dunemud.net"
port = 6788
tls = "Enabled"

[[binding]]
keys = "shift-up"
action = "scrolltop"
```

See [MUDs](./muds.md) for more information on the MUD config fields.

See [Keybindings](./keybindings.md) for more information on the keybinding config fields.