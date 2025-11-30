# Mouse support

You can optionally enable mouse support in Mudpuppy in your `config.toml` with
the global setting `mouse_enabled`. For example:

```toml
# Enable mouse support
mouse_enabled = true
```

Make sure this configuration is outside of any [MUD] or [Keybinding] stanzas in
your config TOML.

<div class="warning">
<strong>Important:</strong> mouse mode often interferes with selecting text to
copy/paste. Support for mouse mode varies by terminal.
</div>

[MUD]: ./muds.md
[Keybinding]: ./keybindings.md

## Mouse Scrolling

When `mouse_enabled` is `true` you can choose whether or not mouse scroll events
are used to scroll the output history scrollback buffer using the global setting
`mouse_scroll`. For example:

```toml
# Enable mouse support, and mouse scrolling of output history
mouse_enabled = true
mouse_scroll = true
```
