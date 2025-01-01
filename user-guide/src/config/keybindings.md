# Key Bindings

Key bindings are a way to map keyboard keys to Mudpuppy shortcut actions. The bindings are configured based on
which tab is currently focused: the Mud list, or a connected MUD session.

## Example

```toml
# Quit a MUD session with 'ctrl-x'
[[keybinding]]
keys = "ctrl-x"
action = "quit"

# Move to the previous MUD on the MUD list tab with 'j'
[[keybinding]]
mode = "mudlist"
keys = "j"
action = "mudlistprev"

# Move to the next MUD on the MUD list tab with 'k'
[[keybinding]]
mode = "mudlist"
keys = "k"
action = "mudlistnext"
```

## Fields

Each key binding is defined in a `[[keybinding]]` TOML table in your config file. The following fields
are available for each key binding:

| Field  | Optional | Type   | Default      | Examples                      |
|--------|----------|--------|--------------|-------------------------------|
| mode   | True     | String | "mudsession" | "mudsession", "mudlist"       |
| keys   | No       | String | N/A          | "ctrl-q", "shift-up", "f4"    |
| action | No       | String | N/A          | "quit", "scrolltop", "toggle" |


### mode

The tab type that must be in-focus for the key binding to be active. 
The default is `mudsession`, meaning the key binding is only active when you're
on a MUD's session tab.

If you want a key binding to be active on the MUD list tab, set 
`mode = "mudlist"` instead.

### keys

The key or key combination that triggers the action. This can include modifiers by
separating them with a `-`. For example, `ctrl-x`, `shift-up`, or `f4`.

#### modifiers

The available modifiers are:

* `ctrl`
* `alt`
* `shift`

#### keys

The available keys are:

* `space` (space bar)
* `enter` (return key)
* `esc` (escape key)
* `tab` (tab key)
* `backspace` (backspace key)
* `delete` (delete key)
* `insert` (insert key)
* `home` (home key)
* `end` (end key)
* `pageup` (page up key)
* `pagedown` (page down key)
* `up` (up arrow key)
* `down` (down arrow key)
* `left` (left arrow key)
* `right` (right arrow key)
* `f1` through `f12` (function keys)
* all other normal singular keys, e.g. 'a-z', '0-9', punctuation, etc.

## action

The shortcut action that will be taken when the `keys` are input. 
For example, `quit`, `scrolltop`, or `toggle`.

#### Shortcuts

The available shortcuts (case insensitive) are:

* `Quit` - Quit the current MUD session
* `TabNext` - Move to the next tab
* `TabPrev` - Move to the previous tab
* `TabClose` - Close the current tab
* `TabSwapLeft` - Swap the current tab with the one to the left
* `TabSwapRight` - Swap the current tab with the one to the right
* `MudListNext` - Move to the next MUD on the MUD list tab
* `MudListPrev` - Move to the previous MUD on the MUD list tab
* `MudListConnect` - Connect to the currently selected MUD on the MUD list tab
* `ToggleLineWrap` - Toggle [line wrapping config](./muds.md#no_line_wrap) for the output buffer
* `ToggleEchoInput` - Toggle [echo input config](./muds.md#echo_input) for the output buffer
* `HistoryNext` - Move to the next input history entry
* `HistoryPrev` - Move to the previous input history entry
* `ScrollUp` - Scroll up in the output buffer
* `ScrollDown` - Scroll down in the output buffer
* `ScrollTop` - Scroll to the top of the output buffer
* `ScrollBottom` - Scroll to the bottom of the output buffer