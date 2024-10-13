# Configuration

**TODO: Write more about config**

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

## Example

```toml
[[muds]]
name = "DuneMUD (TLS)"
host = "dunemud.net"
port = 6788
tls = "Enabled"

[[muds]]
name = "DunemUD (Telnet)"
host = "dunemud.net"
port = 6789
tls = "Disabled"

[[muds]]
name = "Custom"
host = "dunemud.net"
no_tcp_keepalive = true
hold_prompt = false
echo_input = false
no_line_wrap = true
debug_gmcp = true
splitview_percentage = 50
splitview_margin_horizontal = 0
splitview_margin_vertical = 0

# Default key bindings for the MUD list.
[keybindings.MudList.Up]
MudListPrev = {}
[keybindings.MudList.Down]
MudListNext = {}
[keybindings.MudList.Enter]
MudListConnect = {}
[keybindings.MudList."<Ctrl-p>"]
TabPrev = {}
[keybindings.MudList."<Ctrl-n>"]
TabNext = {}
[keybindings.MudList.q]
Quit = {}
[keybindings.MudList.x]
Quit = {}
[keybindings.MudList."<Ctrl-d>"]
Quit = {}
[keybindings.MudList."<Ctrl-c>"]
Quit = {}
[keybindings.MudList."<Ctrl-x>"]
Quit = {}

# Default key bindings for MUD sessions.
[keybindings.Mud."<Ctrl-p>"]
TabPrev = {}
[keybindings.Mud."<Ctrl-n>"]
TabNext = {}
[keybindings.Mud."<Alt-p>"]
TabSwapLeft = {}
[keybindings.Mud."<Alt-n>"]
TabSwapRight = {}
[keybindings.Mud."<Ctrl-d>"]
Quit = {}
[keybindings.Mud."<Ctrl-c>"]
Quit = {}
[keybindings.Mud."<Ctrl-q>"]
Quit = {}
[keybindings.Mud."<Ctrl-x>"]
TabClose = {}
[keybindings.Mud."F2"]
ToggleInputEcho = {}
[keybindings.Mud."F3"]
ToggleLineWrap = {}
[keybindings.Mud."Up"]
HistoryPrevious = {}
[keybindings.Mud."Down"]
HistoryNext = {}
[keybindings.Mud."PageUp"]
ScrollUp = {}
[keybindings.Mud."PageDown"]
ScrollDown = {}
[keybindings.Mud."<Shift-Home>"]
ScrollTop = {}
[keybindings.Mud."<Shift-End>"]
ScrollBottom = {}
```
