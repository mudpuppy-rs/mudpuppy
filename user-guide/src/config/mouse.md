# Mouse support

You can optionally enable mouse support in Mudpuppy in your `config.toml` with
the global setting `mouse_enabled`. For example:

```toml
# Enable mouse support
mouse_enabled = true
```

Make sure this configuration is outside of any [MUD] or [Keybinding] stanzas in
your config TOML.

Note that unlike other settings, you **must** restart Mudpuppy for a change to
`mouse_enabled` to take effect.

<div class="warning">
<strong>Important:</strong> mouse mode often interferes with selecting text to
copy/paste. Support for mouse mode varies by terminal.
</div>

[MUD]: ./muds.md
[Keybinding]: ./keybindings.md
