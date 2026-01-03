"""
The `pup` module provides low-level access to Mudpuppy's core functionality.

This module exposes the Python API for creating and managing MUD client sessions,
handling events, managing buffers and output, and customizing the TUI.

For helpful higher-level function decorators, see `pup_events`.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any, Awaitable, Callable, Optional, Union, TYPE_CHECKING

# Explicitly ordered for documentation readability
__all__ = [
    # Core functions
    "config",
    "config_file",
    "config_dir",
    "data_dir",
    # Session management
    "new_session",
    "active_session",
    "sessions",
    "session",
    "session_for_character",
    "new_session_handler",
    # Tab management
    "tabs",
    "create_tab",
    # Global operations
    "quit",
    "show_error",
    "print",
    "new_floating_window",
    "global_shortcuts",
    "set_global_shortcut",
    # Configuration types
    "Config",
    "Settings",
    "SettingsOverlay",
    "Character",
    "Mud",
    "Tls",
    # Session types
    "Session",
    "Prompt",
    "Telnet",
    "Gmcp",
    "Triggers",
    "Aliases",
    # Tab type
    "Tab",
    # Buffer types
    "Buffer",
    "BufferConfig",
    "BufferDirection",
    "Scrollbar",
    # Output/Input types
    "OutputItem",
    "MudLine",
    "InputLine",
    "Input",
    "EchoState",
    "Markup",
    # Trigger/Alias/Timer types
    "Trigger",
    "Alias",
    "Timer",
    # Event system
    "EventType",
    "Event",
    "Dimensions",
    # Dialog/Window types
    "FloatingWindow",
    "Position",
    "Size",
    "DialogPriority",
    # Layout types
    "Section",
    "Constraint",
    "Direction",
    # Input types
    "KeyEvent",
    # Shortcut types
    "Shortcut",
    "InputShortcut",
    "TabShortcut",
    "MenuShortcut",
    "PythonShortcut",
    # Prompt types
    "PromptMode",
    "PromptSignal",
]

# =============================================================================
# TYPE DEFINITIONS (must come before functions to avoid forward reference issues)
# =============================================================================

# -----------------------------------------------------------------------------
# Configuration types
# -----------------------------------------------------------------------------

class Config:
    """
    Top-level application configuration.

    Contains MUD definitions, character configurations, global settings,
    and Python module loading configuration.

    Mutating the configuration values overrides the values from the config
    file for the duration of application execution, or until the config file
    is changed on disk. At this point a `Event.ConfigReloaded` event is emitted
    and the `Config` is replaced with the values from the `config_file()`.
    """

    mouse_enabled: bool
    """
    **Default**: `True`
    
    Whether the TUI supports mouse interaction. Set to false
    to prevent Mudpuppy from intercepting mouse events (e.g. to allow
    simple text highlighting).
    
    Most terminals allow text selection when mouse mode is enabled by
    holding SHIFT and selecting text. Details vary by terminal.
    """

    confirm_quit: bool
    """
    **Default**: `True`
    
    Whether to confirm before quitting the application.
    
    Beware! If set to `False`, the application will quit immediately 
    for `/quit` slash commands, the quit keyboard shortcuts, etc and all
    active sessions will immediately exit.
    """

    muds: dict[str, Mud]
    """
    A dictionary of MUD names to `Mud` definitions/settings.
    
    Characters in the `Config.characters` dictionary reference MUDs from
    this dictionary by name in the `Character.mud` field.
    
    You can use `Config.mud()` to find a `Mud` definition by name.
    """

    characters: dict[str, Character]
    """
    A dictionary of character names to `Character` definitions/settings.
    
    You can use `Config.character()` to find a `Character` definition by name.
    """

    modules: list[str]
    """
    A list of Python modules that should be imported at application startup.
    
    These modules are useful for doing "global" work like adding character
    definitions to `config.Characters`, or registering slash commands, telnet
    handlers, etc that should operate for all characters.
    
    For per-character modules, see `Character.module`.
    
    **Note:** since these modules are only loaded at application startup,
    changing the list at runtime has no effect.
    """

    settings: Settings
    """
    Global `Settings`.
    
    These setting values are used only if there are no overrides in
    `Mud.settings` or `Character.settings`. 
    
    Generally you should use `Config.resolve_settings()` to get a
    `Settings` with character and MUD specific overrides applied. 
    Use `Config.settings` only if you specifically want to ignore
    `Character.settings` and `Mud.settings`.
    """

    def character(self, name: str) -> Optional[Character]:
        """Get a `Character` configuration by name, if it exists."""
        ...

    def mud(self, name: str) -> Optional[Mud]:
        """Get a `Mud`` configuration by name, if it exists."""
        ...

    def resolve_settings(self, char_name: Optional[str] = None) -> Settings:
        """
        Resolve effective settings for a specific character name, or from
        the global `Config.settings`.

        Merges global, MUD-specific, and character-specific settings. The
        `Settings` values from the `Character.settings` have the highest
        priority, followed by `Mud.settings` for the `Character.mud`, and
        lastly the `Config.settings`.

        ### Args:

        `char_name`: Character name to resolve settings for. If `None`,
        then only global settings are consulted.

        ### Returns:
        The resolved `Settings` with all overrides applied
        """
        ...

    def resolve_extra_setting(
        self,
        char_name: str,
        key: str,
        *,
        default: Optional[str] = None,
    ) -> Optional[str]:
        """
        Resolve a custom extra setting for a character name.

        The built-in `Settings` and `Config.resolve_settings()` are helpful
        for working with Mudpuppy built-in settings. It's also often
        helpful for custom scripts to have their own configuration and to
        allow character/MUD overrides. For this purpose the "extra" settings
        API can be used.

        ### Args:
        * `char_name`: Character name
        * `key`: Setting key to look up
        * `default`: Default value if not found

        ### Returns:
        The setting value, or default if not found.
        """
        ...

class Settings:
    """
    Global settings with per-MUD and per-character override support.

    See `Config.resolve_settings()`, `Character.settings`, `Mud.settings` and `Config.settings` for
    more information.
    """

    word_wrap: bool
    """
    **Default**: `True`
    
    Whether the main output buffer and scrollback buffer should wrap line content
    to fit.
    
    If disabled, long lines may appear truncated if the window isn't wide enough
    to display the full content.
    """

    send_separator: str
    """
    **Default**: `;;` (two semicolons)
    
    A character that can be used to separate a single line of input so it will be split
    into several when sent to the MUD.
    
    For example, sending `wave;;say hello` with the default `send_separator` will send the 
    MUD _two_ lines: `wave` and `say hello`.
    
    If your MUD uses `;` as a special character you may wish to change this setting.
    """

    command_prefix: str
    """
    **Default**: `/` (forward slash)
    
    A character that can be used at the start of an input line to invoke a built-in command.
    
    For example, sending `/connect` with the default `command_prefix` will try to connect
    the current session if it isn't already connected.
    """

    hold_prompt: bool
    """
    **Default**: `True`
    
    When Mudpuppy detects a line is a prompt, it "holds" it at the bottom of the display
    instead of printing it in the MUD output buffer.
    
    If your MUD prompt is misdetected, or you find the behaviour annoying, it can be disabled.
    """

    echo_input: bool
    """
    **Default**: `True`
    
    Whether to print the input you send to the MUD in the output buffer.
    
    If you prefer to only see the output from the commands you send, set this
    setting to `False`.
    """

    scroll_lines: int
    """
    **Default**: 5
    
    How many lines of output to scroll at a time when the scroll up/down
    shortcuts are used. Increase this value to scroll "faster".
    """

    scrollback_percentage: int
    """
    **Default**: 70
    
    What percentage of the output buffer should the scrollback window take up.
    
    Increase this value to see more scrollback, decrease it to show more output
    while scrolling scrollback content.
    """

    scrollback_vertical_margin: int
    """
    **Default**: 0
    
    How many lines of margin should appear between the top of the scrollback window.
    and the output buffer.
    
    Increase this value to move the scrollback window down from the top of the buffer.
    """

    scrollback_horizontal_margin: int
    """
    **Default**: 6
    
    How many columns of margin should appear on the left/right side of the 
    scrollback window.
    
    Increase this value to show more output content to the left/right of the
    scrollback window while scrolling the backlog.
    """

    gmcp_echo: bool
    """
    **Default**: `False`
    
    Whether to display raw received GMCP messages in the output buffer 
    for debugging purposes.
    """

    confirm_close: bool
    """
    **Default**: `True`
    
    Whether to confirm before closing a `Tab`.
    
    **Warning**: if set to `False` pressing the tab close shortcut will close
    the session tab without confirmation, disconnecting from the MUD if 
    connected.
    """

    output_buffer: BufferConfig
    """
    A `BufferConfig` for configuring the behaviour of the main output buffer.
    """

    scrollback_buffer: BufferConfig
    """
    A `BufferConfig` for configuring the behaviour of the scrollback buffer.
    """

    extras: dict[str, str]
    """
    Extra setting key/value pairs that can be set by user Python scripts.
    
    See `Config.resolve_extra_setting()` for more information.
    """

class SettingsOverlay:
    """
    Optional overrides for `Settings`.

    All fields are `Optional` to allow selective overriding of base settings.
    If `None`, then the next layer of `SettingsOverlay`, or the global `Settings`
    value will be used. If not `None`, the setting value is overridden when using
    `Config.resolve_settings()`.
    """

    word_wrap: Optional[bool]
    """
    See `Settings.word_wrap`
    """

    send_separator: Optional[str]
    """
    See `Settings.send_separator`
    """

    command_prefix: Optional[str]
    """
    See `Settings.command_prefix`
    """

    hold_prompt: Optional[bool]
    """
    See `Settings.hold_prompt`
    """

    echo_input: Optional[bool]
    """
    See `Settings.echo_input`
    """

    scroll_lines: Optional[int]
    """
    See `Settings.scroll_lines`
    """

    show_input_echo: Optional[bool]
    """
    See `Settings.show_input_echo`
    """

    scrollback_percentage: Optional[int]
    """
    See `Settings.scrollback_percentage`
    """

    scrollback_vertical_margin: Optional[int]
    """
    See `Settings.scrollback_vertical_margin`
    """

    scrollback_horizontal_margin: Optional[int]
    """
    See `Settings.scrollback_horizontal_margin`
    """

    gmcp_echo: Optional[bool]
    """
    See `Settings.gmcp_echo`
    """

    confirm_close: Optional[bool]
    """
    See `Settings.confirm_close`
    """

    output_buffer: Optional[BufferConfig]
    """
    See `Settings.output_buffer`
    """

    scrollback_buffer: Optional[BufferConfig]
    """
    See `Settings.scrollback_buffer`
    """

    extras: Optional[dict[str, str]]
    """
    See `Settings.extras`
    """

class Character:
    """
    Configuration for a character.

    Characters are uniquely identified by their name in the
    `Config.characters` dict.

    Each character references a `Mud` for the details on how
    to connect to an associated MUD server.
    """

    module: str
    """
    Name of a Python module to load the first time a session is created
    for the `Character`. 
    
    A `setup(sesh: Session)` function can be defined in the specified
    module and it will be called whenever a new session is created for
    the character.
    """

    mud: str
    """
    Name of a MUD from the `Config.muds` dict.
    
    Specifies how to connect to the MUD server for this 
    character.
    """

    settings: SettingsOverlay
    """
    Overrides for `Settings` specific to this character.
    
    Settings changed here will take precedence over 
    both MUD-specific `Mud.settings` and global `Config.settings`.
    
    See also `Mud.settings`, `Config.settings` and `Config.resolve_settings()`.
    """

class Mud:
    """
    Configuration for a MUD server.

    MUDs are uniquely identified by their name in the
    `Config.muds` dict.

    Each `Character` in `Config.characters` referencess
    a `Mud` for the details on how to connect to an associated
    MUD server.
    """

    host: str
    """
    The domain name, or IP address, of the MUD server.
    """

    port: int
    """
    The port to connect to on the MUD server.
    
    Some MUDs offer multiple port options, and may require choosing a
    specific port to use TLS.
    """

    tls: Tls
    """
    **Default**: `Tls.Disabled`
    
    The `Tls` configuration to use to secure the connection.
    """

    settings: SettingsOverlay
    """
    Overrides for `Settings` specific to this MUD.
    
    Settings in `Character.settings` will take precedence over these
    settings. Changes made in these settings will take precedence 
    over global `Config.settings`.
    
    See also `Character.settings`, `Config.settings` and `Config.resolve_settings()`.
    """

class Tls:
    """
    Transport Layer Security (TLS) configuration options.

    Where possible `Tls.Enabled` offers the best security.
    """

    Disabled: Tls
    """
    TLS is **disabled** and only insecure plaintext Telnet is used.
    
    **Warning:** all your data (including your login password) are
    sent unencrypted.
    """

    Enabled: Tls
    """
    **Recommended**
    
    TLS is enabled and certificate validation is always performed.
    """

    DangerouslyAllowInvalidCerts: Tls
    """
    TLS is enabled, but invalid certificates are allowed.
    
    **Warning**: this option is vulnerable to person-in-the-middle 
    attacks and is generally insecure.
    """

# -----------------------------------------------------------------------------
# Buffer types
# -----------------------------------------------------------------------------

class Buffer:
    """
    A buffer for displaying text output.

    Buffers are the fundamental display unit in mudpuppy, containing lines
    of output with scrolling support.

    Buffers can be associated with a `Tab` using `Tab.add_buffer()`, or used
    with a `FloatingWindow`.
    """

    name: str
    """
    A unique name for the buffer.
    
    This is used in events, like `Event.BufferResized` to identify the buffer.
    """

    config: BufferConfig
    """
    Configuration settings for the buffer controlling
    aspects like line wrapping.
    """

    scroll_pos: int
    """
    The current scroll position for displaying the content.
    """

    max_scroll: int
    """
    The maximum possible `scroll_pos` based on the current content
    """

    dimensions: tuple[int, int]
    """
    The current width and height of the buffer.
    """

    def __init__(self, name: str) -> None:
        """
        Create a new buffer.

        ### Args:

        `name`: The buffer name (must be non-empty)
        """
        ...

    def new_data(self) -> int:
        """Get the count of new data items since last read."""
        ...

    def len(self) -> int:
        """Get the total number of items in the buffer."""
        ...

    def add(self, item: OutputItem) -> None:
        """
        Add an output item to the buffer.

        ### Args:
        `item`: The `OutputItem` to add
        """
        ...

    def add_multiple(self, items: list[OutputItem]) -> None:
        """
        Add multiple output items to the buffer.

        ### Args:
        `items`: List of `OutputItem`s to add
        """
        ...

    def scroll(self) -> int:
        """Get the current scroll position."""
        ...

    def scroll_up(self, lines: int) -> None:
        """
        Scroll up by the given number of lines.

        ### Args:
        `lines`: Number of lines to scroll
        """
        ...

    def scroll_down(self, lines: int) -> None:
        """
        Scroll down by the given number of lines.

        ### Args:
        `lines`: Number of lines to scroll
        """
        ...

    def scroll_bottom(self) -> None:
        """Scroll to the bottom of the buffer."""
        ...

    def scroll_to(self, scroll: int) -> None:
        """
        Scroll to a specific position.

        ### Args:
        `scroll`: The scroll position
        """
        ...

    def scroll_max(self) -> None:
        """Scroll to the maximum scroll position."""
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

class BufferConfig:
    """
    Configuration for `Buffer` display behavior.
    """

    line_wrap: bool
    """
    **Default**: `True`
    
    Whether `OutputItem`s in the buffer should be wrapped when the
    content doesn't fit in the `Buffer.dimensions`.
    
    If `False`, long lines may be truncated.
    """

    border_top: bool
    """
    **Default**: `True`
    
    Whether to draw a top border for the buffer.
    """

    border_bottom: bool
    """
    **Default**: `True`
    
    Whether to draw a bottom border for the buffer.
    """

    border_left: bool
    """
    **Default**: `True`
    
    Whether to draw a left border for the buffer.
    """

    border_right: bool
    """
    **Default**: `True`
    
    Whether to draw a right border for the buffer.
    """

    direction: BufferDirection
    """
    **Default**: `BufferDirection.BottomToTop`
    
    How the buffer contents should flow. 
    """

    scrollbar: Scrollbar
    """
    **Default**: `Scrollbar.IfScrolled`
    
    Controls how/when a scrollbar is rendered for the buffer.
    """

    def all_borders(self) -> None:
        """Enable all borders (top, bottom, left, right)."""
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

class BufferDirection:
    """
    Direction for buffer content flow for `BufferConfig`.
    """

    TopToBottom: BufferDirection
    """
    The buffer content is rendered top down, with the oldest
    items at the top of the buffer, and the scroll at 0. Newer
    content may be out of view once there are more items than
    space available.
    """

    BottomToTop: BufferDirection
    """
    **Default**
    
    The buffer content is rendered bottom up, with the 
    newest items at the bottom of the buffer, and the
    scroll at maximum. This operates the same way as the 
    main MUD output buffer. Older content may be out of view
    once there are more items than space available.
    """

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...

class Scrollbar:
    """
    Scrollbar display mode for `BufferConfig`.
    """

    IfScrolled: Scrollbar
    """
    **Default**
    
    A scrollbar is only drawn if the `Buffer` has been scrolled
    by changing the scroll position.
    """

    Never: Scrollbar
    """
    A scrollbar is never drawn, even if the `Buffer` scroll position
    has been changed.
    """

    Always: Scrollbar
    """
    A scrollbar is always drawn, even if the `Buffer` has not been
    scrolled.
    """

# -----------------------------------------------------------------------------
# Output/Input types
# -----------------------------------------------------------------------------

class OutputItem:
    """
    An item of output to display in a `Buffer`.

    Can represent MUD output, user input, prompts, debug messages, etc.

    Items are added to a `Buffer` with `Buffer.add_item()`.
    """

    @staticmethod
    def mud(line: MudLine) -> OutputItem:
        """
        Create an output item from MUD server data.

        ### Args:
        `line`: The `MudLine` to display as output from a MUD.
        """
        ...

    @staticmethod
    def input(line: InputLine) -> OutputItem:
        """
        Create an output item from user input.

        ### Args:
        `line`: The `InputLine` to display as input from a user.
        """
        ...

    @staticmethod
    def command_result(message: str) -> OutputItem:
        """
        Create an output item for a successful command result.

        ### Args:
        `message`: The result message
        """
        ...

    @staticmethod
    def failed_command_result(message: str) -> OutputItem:
        """
        Create an output item for a failed command result.

        ### Args:
        `message`: The error message
        """
        ...

    @staticmethod
    def debug(line: str) -> OutputItem:
        """
        Create a debug output item.

        ### Args:
        `line`: The debug message
        """
        ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

class MudLine:
    """
    A line of text received from a MUD server.
    """

    prompt: bool
    """
    **Default**: `False`
    
    Indicates whether the line was received as a prompt.
    
    Depending on `PromptMode` this could mean a line without a line ending
    or an explicitly signalled prompt line.
    """

    gag: bool
    """
    **Default**: `False`
    
    When `True`, the line is not displayed in the buffer. 
    It's "gagged", and hidden from output.
    
    A `Trigger` may gag a line based on patterns to hide
    output the user doesn't wish to see (e.g. verbose 
    information, etc).
    """

    def __init__(self, value: bytes) -> None:
        """
        Create a MudLine from raw bytes.

        ### Args:
        `value`: The raw byte data
        """
        ...

    def stripped(self) -> str:
        """
        Get the line with ANSI escape codes stripped.

        This removes any colour information that may be present.
        `Trigger`s may operate on stripped lines to make writing
        patterns easier.
        """
        ...

    def set(self, value: str) -> None:
        """
        Replace the line content with a new value.

        ### Args:
        `value`: The new content
        """
        ...

    def raw(self) -> bytes:
        """
        Get the raw byte representation of the line.
        """
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

class InputLine:
    """
    A line of user input.
    """

    sent: str
    """
    The line of data that was, or will be, transmitted to the server.
    
    It may not be the original input the user typed. E.g. if an `Alias`
    replaced the input, then `sent` will be the replacement value and
    `original` is what the user originally sent.
    """

    original: Optional[str]
    """
    If `sent` was replaced, for example by an `Alias`, then `original`
    will hold the value that was replaced.
    """

    echo: EchoState
    """
    The `EchoState` at the time the input was produced.
    """

    scripted: bool
    """
    If `False`, the input was produced by the user. If `True` it
    was produced (e.g. using `session.send_line()`), or modified  
    (e.g. using an `Alias`) by a script and not the user.
    """

    def __init__(
        self,
        sent: str,
        original: Optional[str] = None,
        echo: Optional[EchoState] = None,
        scripted: bool = False,
    ) -> None: ...
    """
    Create a new `InputLine` for the `sent` content.
    
    Optionally, the `original` content, `echo`, and 
    `scripted` status may be provided.
    """

    def empty(self) -> bool:
        """Check if the input line is empty (whitespace only)."""
        ...

    def split(self, sep: str) -> list[InputLine]:
        """
        Split the input line by a separator.

        ### Args:
        `sep`: The separator string

        ### Returns:
        List of `InputLines` created from the split.

        The `original`, `echo` and `scripted` values from the
        split up `InputLine` are preserved on the split portions.
        """
        ...

    def clone_with_original(self) -> InputLine:
        """
        Clone the line but swap `sent` and `original` fields.
        """
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

class Input:
    """
    Current input state for a session.

    Represents the input text area of the TUI.
    """

    def cursor(self) -> int:
        """Get the current cursor position."""
        ...

    def visual_cursor(self) -> int:
        """
        Get the current visual cursor position accounting
        for character width.
        """
        ...

    def visual_scroll(self, width: int) -> int:
        """
        Calculate scroll offset for the given width.

        ### Args:
        `width`: The display width

        ### Returns:
        The scroll offset
        """
        ...

    def reset(self) -> None:
        """Reset the input to empty."""
        ...

    def pop(self) -> Optional[InputLine]:
        """
        Pop the current input line.

        This returns an `InputLine` and resets the content to empty.
        To get the value without clearing, use `value()`.

        ### Returns:
        The `InputLine` if non-empty, `None` otherwise
        """
        ...

    def echo(self) -> EchoState:
        """Get the current echo state."""
        ...

    def markup(self) -> Markup:
        """Get the markup manager for styling input text."""
        ...

    def value(self) -> InputLine:
        """
        Get the current input value as an `InputLine`.

        Unlike `pop()` this does not clear the content.
        """
        ...

    def set_value(self, line: InputLine) -> None:
        """
        Set the input value.

        Replaces the current content with a new `InputLine`.

        ### Args:
        `line`: The `InputLine` to set as the current input
        """
        ...

    def __str__(self) -> str: ...

class EchoState:
    """
    Echo state for `Input` display.

    This is typically changed based on telnet option negotiation.
    `InputLines` produced from the `Input` remember the `EchoState`
    that the `Input` was in when the `InputLine` was produced.
    """

    Normal: EchoState
    """
    **Default**
    
    Input is displayed normally.
    """

    Password: EchoState
    """
    Input is displayed masked out, like a password.
    
    This typically corresponds to Telnet "no echo".
    """

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...

class Markup:
    """
    ANSI markup/styling for `Input` text.

    Markup allows applying style to an `InputLine` that won't affect
    the content sent to the MUD server. It's only used when displaying
    the `InputLine` in the `Input` area.
    """

    def add(self, pos: int, token: str) -> None:
        """
        Add a markup token at a position.

        ### Args:
        * `pos`: Character position to insert the markup when rendering.
        * `token`: ANSI escape sequence or markup string.
        """
        ...

    def remove(self, pos: int) -> None:
        """
        Remove markup at a position previously set with `add()`.

        ### Args:
        `pos`: Character position
        """
        ...

    def clear(self) -> None:
        """Clear all markup."""
        ...

    def __repr__(self) -> str: ...

# -----------------------------------------------------------------------------
# Trigger/Alias/Timer types
# -----------------------------------------------------------------------------

class Trigger:
    """
    Pattern-based trigger for responding to MUD output.
    """

    name: str
    """
    The name of the `Trigger`
    """

    enabled: bool
    """
    **Default**: `True`
    
    Whether the `Trigger` is currently active. If `False`, it will be
    ignored even if output lines match the trigger pattern.
    """

    strip_ansi: bool
    """
    **Default**: `False`
    
    Whether `MudLine`s should have ANSI stripped before
    trying to match the pattern. This is helpful for writing triggers
    that don't need to account for ANSI styling in the pattern.
    
    If you want to match text with specific ANSI tokens (e.g. text of
    a certain colour) you must leave this `False`.
    """

    prompt: bool
    """
    **Default**: `False`
    
    Whether the trigger should only match `MudLines` that were detected
    as a prompt line.
    """

    gag: bool
    """
    **Default**: `False`
    
    Whether `MudLine`s that match the trigger should be gagged (e.g.
    hidden from output).
    """

    callback: Optional[
        Callable[[Session, Trigger, MudLine, Optional[list[str]]], Awaitable[None]]
    ]
    """
    An optional async handler function to be called when the trigger matches.
    
    ### Handler Args:
    * `sesh` - the `Session` that received the matching line.
    * `trigger` - the `Trigger` that matched.
    * `line` - the `MudLine` that matched the trigger pattern.
    * `matches` - an optional list of matches from the trigger pattern extracted from `line`.
    """

    highlight: Optional[
        Callable[[Session, Trigger, MudLine, Optional[list[str]]], MudLine]
    ]
    """
    An optional **non-async** handler function to be called when the trigger matches.
    
    This handler can return a `MudLine` used to **replace** the matched `MudLine`.
    E.g. to apply colour, or to replace the content entirely.
    
    ### Handler Args:
    * `sesh` - the `Session` that received the matching line.
    * `trigger` - the `Trigger` that matched.
    * `line` - the `MudLine` that matched the trigger pattern.
    * `matches` - an optional list of matches from the trigger pattern extracted from `line`.
    
    ### Handler Return:
    A new `MudLine` to use to replace the matched `MudLine`.
    """

    reaction: Optional[str]
    """
    An optional "reaction" that will be sent as a scripted `InputLine` to the MUD server when
    an output line matches the trigger pattern.
    """

    hit_count: int
    """
    A count of the number of times the trigger matched an output line.
    """

    def __init__(
        self,
        pattern: str,
        name: str,
        *,
        strip_ansi: bool = False,
        prompt: bool = False,
        gag: bool = False,
        callback: Optional[
            Callable[[Session, Trigger, MudLine, Optional[list[str]]], Awaitable[None]]
        ] = None,
        highlight: Optional[
            Callable[[Session, Trigger, MudLine, Optional[list[str]]], MudLine]
        ] = None,
        reaction: Optional[str] = None,
    ) -> None:
        """
        Create a new trigger.

        ### Args:
        * `pattern`: Regular expression pattern to match
        * `name`: Unique name for this trigger
        * `strip_ansi`: Strip ANSI codes before matching (default: `False`)
        * `prompt`: Only trigger on prompt lines (default: `False`)
        * `gag`: Prevent the line from being displayed (default: `False`)
        * `callback`: Async function to call on match (default: `None`)
        * `highlight`: Function to modify the line on match (default: `None`)
        * `reaction`: Text to send to the MUD on match (default: `None`)
        """
        ...

    def pattern(self) -> str:
        """Get the trigger's regex pattern string."""
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

class Alias:
    """
    Pattern-based alias for transforming user input.
    """

    name: str
    """
    The name of the `Alias`.
    """

    enabled: bool
    """
     **Default**: `True`
     
     Whether the `Alias` is currently active. If `False`, it will be
     ignored even if input lines match the alias pattern.
     """

    callback: Optional[
        Callable[[Session, Alias, InputLine, Optional[list[str]]], Awaitable[None]]
    ]

    reaction: Optional[str]
    hit_count: int

    def __init__(
        self,
        pattern: str,
        name: str,
        *,
        callback: Optional[
            Callable[[Session, Alias, InputLine, Optional[list[str]]], Awaitable[None]]
        ] = None,
        reaction: Optional[str] = None,
    ) -> None:
        """
        Create a new alias.

        Args:
            pattern: Regular expression pattern to match
            name: Unique name for this alias
            callback: Async function to call on match
            reaction: Replacement text to send on match
        """
        ...

    def pattern(self) -> str:
        """Get the alias's regex pattern."""
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

class Timer:
    """
    Timer that executes a callback at regular intervals.
    """

    name: str
    duration: int
    callback: Optional[Callable[[Timer], Awaitable[None]]]
    reaction: Optional[str]
    session: Optional[Session]
    hit_count: int

    def __init__(
        self,
        name: str,
        duration_seconds: float,
        *,
        callback: Optional[Callable[[Timer], Awaitable[None]]] = None,
        reaction: Optional[str] = None,
        session: Optional[Session] = None,
        start: bool = True,
    ) -> None:
        """
        Create a new timer.

        Args:
            name: Unique name for this timer
            duration_seconds: Interval in seconds (supports fractions)
            callback: Async function to call on each tick
            reaction: Text to send on each tick (requires session)
            session: Session for reaction output
            start: Start the timer immediately
        """
        ...

    def running(self) -> bool:
        """Check if the timer is currently running."""
        ...

    def start(self) -> None:
        """Start the timer."""
        ...

    def stop(self) -> None:
        """Stop the timer."""
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

# -----------------------------------------------------------------------------
# Event system
# -----------------------------------------------------------------------------

class EventType:
    """
    Type identifier for events.
    """

    All: EventType
    ConfigReloaded: EventType
    SessionClosed: EventType
    SessionConnecting: EventType
    SessionConnected: EventType
    SessionDisconnected: EventType
    ActiveSessionChanged: EventType
    TelnetOptionEnabled: EventType
    TelnetOptionDisabled: EventType
    TelnetIacCommand: EventType
    TelnetSubnegotiation: EventType
    PromptChanged: EventType
    PromptModeChanged: EventType
    Line: EventType
    InputChanged: EventType
    InputLine: EventType
    BufferResized: EventType
    TabClosed: EventType
    GmcpEnabled: EventType
    GmcpDisabled: EventType
    GmcpMessage: EventType

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    @staticmethod
    def all() -> dict[str, EventType]:
        """Get a mapping of all event type names to their instances."""
        ...

class Event:
    """
    Base event type. Events are tagged unions with specific variants.

    Check event.type() to determine the variant, then access variant-specific
    fields directly (e.g., ev.config for ConfigReloaded, ev.line for Line, etc.)

    Variant-specific fields (only present on certain event types):
    - config: Config (ConfigReloaded)
    - info: dict[str, Any] (SessionConnected)
    - changed_from, changed_to: Optional[Session] (ActiveSessionChanged)
    - option: int (TelnetOptionEnabled, TelnetOptionDisabled, TelnetSubnegotiation)
    - command: int (TelnetIacCommand)
    - data: bytes (TelnetSubnegotiation)
    - from_: str | PromptMode | Dimensions (PromptChanged, PromptModeChanged, BufferResized)
    - to: str | PromptMode | Dimensions (PromptChanged, PromptModeChanged, BufferResized)
    - line: MudLine | InputLine (Line, InputChanged, InputLine)
    - input: Input (InputChanged)
    - name: str (BufferResized)
    - title: str (TabClosed)
    - tab_id: int (TabClosed)
    - package: str (GmcpMessage)
    - json: str (GmcpMessage)
    """

    # Variant-specific fields (all optional as they depend on the event type)
    config: Config
    info: dict[str, Any]
    changed_from: Optional[Session]
    changed_to: Optional[Session]
    option: int
    command: int
    data: bytes
    from_: Union[str, PromptMode, Dimensions]
    to: Union[str, PromptMode, Dimensions]
    line: Union[MudLine, InputLine]
    input: Input
    name: str
    title: str
    tab_id: int
    package: str
    json: str

    # Variant types (for isinstance checks or documentation)
    class ConfigReloaded:
        config: Config

    class SessionClosed: ...
    class SessionConnecting: ...

    class SessionConnected:
        info: dict[str, Any]

    class SessionDisconnected: ...

    class ActiveSessionChanged:
        changed_from: Optional[Session]
        changed_to: Optional[Session]

    class TelnetOptionEnabled:
        option: int

    class TelnetOptionDisabled:
        option: int

    class TelnetIacCommand:
        command: int

    class TelnetSubnegotiation:
        option: int
        data: bytes

    class PromptChanged:
        from_: str  # Note: 'from' is a Python keyword
        to: str

    class PromptModeChanged:
        from_: PromptMode
        to: PromptMode

    class Line:
        line: MudLine

    class InputChanged:
        line: InputLine
        input: Input

    class InputLine:
        line: InputLine

    class BufferResized:
        name: str
        from_: Dimensions
        to: Dimensions

    class TabClosed:
        title: str
        tab_id: int

    class GmcpEnabled: ...
    class GmcpDisabled: ...

    class GmcpMessage:
        package: str
        json: str

    def type(self) -> EventType:
        """Get the event type."""
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

class Dimensions:
    """
    Width and height dimensions.
    """

    def width(self) -> int: ...
    def height(self) -> int: ...
    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...

# -----------------------------------------------------------------------------
# Dialog/Window types
# -----------------------------------------------------------------------------

class FloatingWindow:
    """
    A floating window with buffer-backed content.
    """

    title: Optional[str]
    position: Position
    size: Size
    buffer: Buffer

    def __init__(
        self,
        buffer: Buffer,
        position: Position,
        size: Size,
        title: Optional[str] = None,
    ) -> None: ...

class Position:
    """
    Position for floating windows.
    """

    @staticmethod
    def percent(x: int, y: int) -> Position:
        """
        Create a percentage-based position.

        Args:
            x: X position as percentage (0-100)
            y: Y position as percentage (0-100)
        """
        ...

    @staticmethod
    def absolute(x: int, y: int) -> Position:
        """
        Create an absolute cell-based position.

        Args:
            x: X position in cells
            y: Y position in cells
        """
        ...

class Size:
    """
    Size for floating windows.
    """

    @staticmethod
    def percent(width: int, height: int) -> Size:
        """
        Create a percentage-based size.

        Args:
            width: Width as percentage (0-100)
            height: Height as percentage (0-100)
        """
        ...

    @staticmethod
    def absolute(width: int, height: int) -> Size:
        """
        Create an absolute cell-based size.

        Args:
            width: Width in cells
            height: Height in cells
        """
        ...

class DialogPriority:
    """
    Display priority for dialogs and floating windows.
    """

    Low: DialogPriority
    Normal: DialogPriority
    High: DialogPriority

    def __eq__(self, other: object) -> bool: ...

# -----------------------------------------------------------------------------
# Layout types
# -----------------------------------------------------------------------------

class Section:
    """
    A section in a tab's layout tree.

    Sections can contain child sections to create complex layouts.
    """

    name: str
    direction: Direction
    margin: int
    children: list[tuple[Constraint, Section]]

    def __init__(self, name: str) -> None: ...
    def insert_child(
        self,
        position: int,
        constraint: Constraint,
        section: Section,
    ) -> None:
        """
        Insert a child section at a position.

        Args:
            position: Index to insert at
            constraint: Size constraint for the child
            section: The child Section
        """
        ...

    def append_child(self, constraint: Constraint, section: Section) -> None:
        """
        Append a child section.

        Args:
            constraint: Size constraint for the child
            section: The child Section
        """
        ...

    def set_constraint(self, name: str, constraint: Constraint) -> None:
        """
        Update the constraint for a named child section.

        Args:
            name: Name of the child section
            constraint: New constraint
        """
        ...

    def get_constraint(self, name: str) -> Optional[Constraint]:
        """
        Get the constraint for a named section.

        Args:
            name: Section name

        Returns:
            The Constraint, or None if not found
        """
        ...

    def get_parent(self, name: str) -> Optional[Section]:
        """
        Find the parent section of a named child.

        Args:
            name: Child section name

        Returns:
            The parent Section, or None if not found
        """
        ...

    def tree_string(self) -> str:
        """Get a string representation of the layout tree."""
        ...

if TYPE_CHECKING:
    _ConstraintBase = Constraint  # type: ignore  # noqa: F821
else:
    _ConstraintBase = object

class Constraint:
    """
    Size constraint for layout sections.

    This is a tagged union type. Construct instances using Constraint.Min(), Constraint.Max(), etc.
    Pattern match on the instance to determine the variant.
    """

    # Nested classes for construction and pattern matching
    class Min(_ConstraintBase):
        """Minimum constraint."""

        __match_args__ = ("rows",)
        rows: int
        def __init__(self, rows: int) -> None: ...

    class Max(_ConstraintBase):
        """Maximum constraint."""

        __match_args__ = ("rows",)
        rows: int
        def __init__(self, rows: int) -> None: ...

    class Length(_ConstraintBase):
        """Fixed length constraint."""

        __match_args__ = ("rows",)
        rows: int
        def __init__(self, rows: int) -> None: ...

    class Percentage(_ConstraintBase):
        """Percentage-based constraint."""

        __match_args__ = ("percent",)
        percent: int
        def __init__(self, percent: int) -> None: ...

    class Ratio(_ConstraintBase):
        """Ratio-based constraint."""

        __match_args__ = ("numerator", "denominator")
        numerator: int
        denominator: int
        def __init__(self, numerator: int, denominator: int) -> None: ...

    class Fill(_ConstraintBase):
        """Fill remaining space."""

        __match_args__ = ()
        def __init__(self) -> None: ...

    # Fields present on instances (depending on variant)
    rows: int  # Present on Min, Max, Length
    percent: int  # Present on Percentage
    numerator: int  # Present on Ratio
    denominator: int  # Present on Ratio

class Direction:
    """
    Layout direction for sections.
    """

    Horizontal: Direction
    Vertical: Direction

# -----------------------------------------------------------------------------
# Input types
# -----------------------------------------------------------------------------

class KeyEvent:
    """
    A keyboard event.
    """

    code: str
    modifiers: list[str]

    def __init__(self, event: str) -> None:
        """
        Create a KeyEvent from a string representation.

        Args:
            event: Key string like "Ctrl-x", "Alt-Shift-f", "PageUp", etc.
        """
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

# -----------------------------------------------------------------------------
# Shortcut types
# -----------------------------------------------------------------------------

class Shortcut:
    """
    A keyboard shortcut action (union of various shortcut types).
    """

    # Note: In the Rust API this is an enum, but from Python's perspective
    # it can be constructed from any of the constituent shortcut types.

    @staticmethod
    def Python(shortcut: PythonShortcut) -> Shortcut:
        """
        Create a Shortcut from a PythonShortcut.

        Args:
            shortcut: The PythonShortcut to wrap

        Returns:
            A Shortcut instance
        """
        ...

class InputShortcut:
    """
    Shortcuts for input editing.
    """

    Send: InputShortcut
    CursorLeft: InputShortcut
    CursorRight: InputShortcut
    CursorToStart: InputShortcut
    CursorToEnd: InputShortcut
    CursorWordLeft: InputShortcut
    CursorWordRight: InputShortcut
    DeletePrev: InputShortcut
    DeleteNext: InputShortcut
    CursorDeleteWordLeft: InputShortcut
    CursorDeleteWordRight: InputShortcut
    CursorDeleteToEnd: InputShortcut
    Reset: InputShortcut

class TabShortcut:
    """
    Shortcuts for tab management.
    """

    @staticmethod
    def SwitchToNext() -> TabShortcut: ...
    @staticmethod
    def SwitchToPrevious() -> TabShortcut: ...
    @staticmethod
    def SwitchToList() -> TabShortcut: ...
    @staticmethod
    def SwitchTo(tab_id: int) -> TabShortcut: ...
    @staticmethod
    def SwitchToSession(session: int) -> TabShortcut: ...
    @staticmethod
    def MoveLeft(tab_id: Optional[int] = None) -> TabShortcut: ...
    @staticmethod
    def MoveRight(tab_id: Optional[int] = None) -> TabShortcut: ...
    @staticmethod
    def Close(tab_id: Optional[int] = None) -> TabShortcut: ...

class MenuShortcut:
    """
    Shortcuts for menu navigation.
    """

    Up: MenuShortcut
    Down: MenuShortcut
    Connect: MenuShortcut

class PythonShortcut:
    """
    Custom Python-defined shortcut.
    """

    def __init__(
        self,
        awaitable: Callable[[KeyEvent, Optional[Session], Tab], Awaitable[None]],
    ) -> None:
        """
        Create a Python shortcut.

        Args:
            awaitable: Async function to execute when shortcut is triggered
        """
        ...

# -----------------------------------------------------------------------------
# Prompt types
# -----------------------------------------------------------------------------

class PromptMode:
    """
    Prompt detection mode.
    """

    Disabled: PromptMode
    EscapeGt: PromptMode
    Telnet: PromptMode
    TelnetAndEscapeGt: PromptMode

class PromptSignal:
    """
    Prompt signal type.
    """

    GA: PromptSignal
    EOR: PromptSignal
    EscGt: PromptSignal

    def __eq__(self, other: object) -> bool: ...

# -----------------------------------------------------------------------------
# Session types
# -----------------------------------------------------------------------------

class Session:
    """
    Represents a connection to a MUD server for a specific character.

    Sessions are the primary way to interact with MUD connections, send/receive
    data, and register event handlers.
    """

    id: int
    character: str

    def __init__(self, id: int, character: str) -> None: ...
    async def connect(self) -> None:
        """Connect this session to its configured MUD server."""
        ...

    def disconnect(self) -> None:
        """Disconnect this session from the MUD server."""
        ...

    def close(self) -> None:
        """Close this session completely."""
        ...

    async def character_config(self) -> Character:
        """Get the character configuration for this session."""
        ...

    async def mud_config(self) -> Mud:
        """Get the MUD configuration for this session."""
        ...

    async def connection_info(self) -> dict[str, Any]:
        """Get connection information for this session."""
        ...

    def set_active(self) -> None:
        """Make this session the active session."""
        ...

    def send_line(
        self,
        line: Union[str, InputLine],
        skip_aliases: bool = False,
    ) -> None:
        """
        Send a line of input to the MUD server.

        Args:
            line: The line to send (str or InputLine)
            skip_aliases: If True, bypass alias processing
        """
        ...

    def send_key(self, key: KeyEvent) -> None:
        """
        Send a key event to this session's input.

        Args:
            key: The KeyEvent to send
        """
        ...

    def output(self, items: Union[OutputItem, list[OutputItem]]) -> None:
        """
        Add output items to this session's output buffer.

        Args:
            items: Single OutputItem or list of OutputItems to display
        """
        ...

    async def input(self) -> Input:
        """Get the current input state for this session."""
        ...

    def add_event_handler(
        self,
        event_type: EventType,
        awaitable: Callable[[Session, Event], Awaitable[None]],
    ) -> None:
        """
        Register an event handler for this session.

        Args:
            event_type: The type of events to handle
            awaitable: Async function called when the event occurs
        """
        ...

    def prompt(self) -> Prompt:
        """Get the prompt manager for this session."""
        ...

    def telnet(self) -> Telnet:
        """Get the telnet protocol manager for this session."""
        ...

    def gmcp(self) -> Gmcp:
        """Get the GMCP manager for this session."""
        ...

    def triggers(self) -> Triggers:
        """Get the trigger manager for this session."""
        ...

    def aliases(self) -> Aliases:
        """Get the alias manager for this session."""
        ...

    async def tab(self) -> Tab:
        """Get the tab containing this session."""
        ...

    async def get_buffers(self) -> list[Buffer]:
        """Get all buffers associated with this session."""
        ...

    def add_slash_command(
        self,
        name: str,
        callback: Callable[[str, Session], Awaitable[None]],
    ) -> None:
        """
        Add a custom slash command for this session.

        Args:
            name: The command name (without the leading slash)
            callback: Async function to execute when command is invoked
        """
        ...

    async def slash_command_exists(self, name: str) -> bool:
        """
        Check if a slash command exists.

        Args:
            name: The command name to check

        Returns:
            True if the command exists
        """
        ...

    def remove_slash_command(self, name: str) -> None:
        """
        Remove a slash command.

        Args:
            name: The command name to remove
        """
        ...

    def new_floating_window(
        self,
        window: FloatingWindow,
        *,
        id: Optional[str] = None,
        dismissible: bool = True,
        priority: DialogPriority = ...,
        timeout: Optional[float] = None,
    ) -> None:
        """
        Display a session-specific floating window.

        Args:
            window: The FloatingWindow to display
            id: Optional identifier for the window
            dismissible: Whether the window can be dismissed
            priority: Display priority
            timeout: Optional timeout in seconds
        """
        ...

    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __hash__(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...

class Prompt:
    """
    Manages prompt detection and handling for a session.
    """

    id: int

    def flush(self) -> None:
        """Flush any buffered prompt data."""
        ...

    async def get(self) -> str:
        """Get the current prompt text."""
        ...

    async def set(self, prompt: str) -> None:
        """
        Set the prompt text.

        Args:
            prompt: The new prompt text
        """
        ...

    async def mode(self) -> PromptMode:
        """Get the current prompt mode."""
        ...

    async def set_mode(self, mode: PromptMode) -> None:
        """
        Set the prompt mode.

        Args:
            mode: The new PromptMode
        """
        ...

class Telnet:
    """
    Manages telnet protocol negotiation for a session.
    """

    id: int

    def request_enable_option(self, option: int) -> None:
        """
        Request to enable a telnet option.

        Args:
            option: The telnet option code
        """
        ...

    def request_disable_option(self, option: int) -> None:
        """
        Request to disable a telnet option.

        Args:
            option: The telnet option code
        """
        ...

    def send_subnegotiation(self, option: int, data: bytes) -> None:
        """
        Send a telnet subnegotiation.

        Args:
            option: The telnet option code
            data: The subnegotiation data
        """
        ...

class Gmcp:
    """
    Manages Generic Mud Communication Protocol for a session.
    """

    id: int

    def register(self, module: str) -> None:
        """
        Register interest in a GMCP module.

        Args:
            module: The GMCP module name to register
        """
        ...

    def unregister(self, module: str) -> None:
        """
        Unregister from a GMCP module.

        Args:
            module: The GMCP module name to unregister
        """
        ...

    def send(self, package: str, json: str) -> None:
        """
        Send a GMCP message.

        Args:
            package: The GMCP package name
            json: JSON-encoded message data
        """
        ...

class Triggers:
    """
    Manages triggers for a session.
    """

    id: int

    def add(self, trigger: Trigger) -> None:
        """
        Add a trigger.

        Args:
            trigger: The Trigger to add
        """
        ...

    def remove(self, trigger: Trigger) -> None:
        """
        Remove a trigger.

        Args:
            trigger: The Trigger to remove
        """
        ...

    async def get(self) -> list[Trigger]:
        """Get all registered triggers."""
        ...

class Aliases:
    """
    Manages aliases for a session.
    """

    id: int

    def add(self, alias: Alias) -> None:
        """
        Add an alias.

        Args:
            alias: The Alias to add
        """
        ...

    def remove(self, alias: Alias) -> None:
        """
        Remove an alias.

        Args:
            alias: The Alias to remove
        """
        ...

    async def get(self) -> list[Alias]:
        """Get all registered aliases."""
        ...

# -----------------------------------------------------------------------------
# Tab type
# -----------------------------------------------------------------------------

class Tab:
    """
    Represents a tab in the mudpuppy UI.

    Tabs can contain sessions or be custom tabs with user-defined layouts.
    """

    id: int

    def set_active(self) -> None:
        """Make this tab the active tab."""
        ...

    async def layout(self) -> Section:
        """Get the layout structure for this tab."""
        ...

    async def title(self) -> str:
        """Get the title of this tab."""
        ...

    async def shortcuts(self) -> dict[str, Shortcut]:
        """Get all keyboard shortcuts defined for this tab."""
        ...

    def set_shortcut(
        self,
        key_event: Optional[Union[KeyEvent, str]],
        shortcut: Optional[
            Union[
                Shortcut, Callable[[KeyEvent, Optional[Session], Tab], Awaitable[None]]
            ]
        ],
    ) -> None:
        """
        Set a keyboard shortcut for this tab.

        Args:
            key_event: The key to bind (KeyEvent or string like "Ctrl-x"), or None
            shortcut: The action to execute, or None to remove the binding
        """
        ...

    def add_buffer(self, buff: Buffer) -> None:
        """
        Add a buffer to this tab.

        Args:
            buff: The Buffer to add
        """
        ...

    async def get_buffer(self, name: str) -> Optional[Buffer]:
        """
        Get a buffer by name.

        Args:
            name: The buffer name

        Returns:
            The Buffer, or None if not found
        """
        ...

    async def get_buffers(self) -> list[Buffer]:
        """Get all buffers in this tab."""
        ...

    def set_title(self, title: str) -> None:
        """
        Set the tab title.

        Args:
            title: The new title
        """
        ...

    def switch_next(self) -> None:
        """Switch to the next tab."""
        ...

    def switch_previous(self) -> None:
        """Switch to the previous tab."""
        ...

    def switch_to_list(self) -> None:
        """Open the tab list selector."""
        ...

    def move_left(self) -> None:
        """Move this tab left in the tab bar."""
        ...

    def move_right(self) -> None:
        """Move this tab right in the tab bar."""
        ...

    def close(self) -> None:
        """Close this tab."""
        ...

# =============================================================================
# MODULE-LEVEL FUNCTIONS (come after types to avoid forward reference issues)
# =============================================================================

async def config() -> Config:
    """
    Get the current configuration.

    Note that when the configuration changes, old Config instances are not
    automatically updated. Use an EventType.ConfigReloaded event handler to
    respond to configuration changes.
    """
    ...

def config_file() -> Path:
    """Get the path to the mudpuppy configuration file in the `config_dir()`."""
    ...

def config_dir() -> Path:
    """
    Get the path to the mudpuppy configuration directory.

    This directory is also used for your Mudpuppy Python scripts.
    The Mudpuppy configuration TOML file path is also located under
    this directory (see `config_file()`).
    """
    ...

def data_dir() -> Path:
    """Get the path to the mudpuppy data directory."""
    ...

def quit() -> None:
    """Quit the mudpuppy application."""
    ...

def show_error(message: str) -> None:
    """Display an error dialog with the given message."""
    ...

async def new_session(character: str) -> Session:
    """
    Create a new session for the given character.

    Args:
        character: The name of the character to create a session for

    Returns:
        The newly created Session
    """
    ...

async def active_session() -> Optional[Session]:
    """Get the currently active session, or None if no session is active."""
    ...

async def sessions() -> list[Session]:
    """Get a list of all active sessions."""
    ...

async def session(session_id: int) -> Optional[Session]:
    """
    Get a session by its ID.

    Args:
        session_id: The numeric ID of the session

    Returns:
        The Session with the given ID, or None if not found
    """
    ...

async def session_for_character(character: str) -> Optional[Session]:
    """
    Get a session for the given character name.

    Args:
        character: The name of the character

    Returns:
        The Session for the character, or None if not found
    """
    ...

def new_session_handler(awaitable: Callable[[Session], Awaitable[None]]) -> None:
    """
    Register a handler to be called when a new session is created.

    Args:
        awaitable: An async function that takes a Session as its argument
    """
    ...

async def tabs() -> list[Tab]:
    """Get a list of all open tabs."""
    ...

async def create_tab(
    title: str,
    *,
    layout: Optional[Section] = None,
    buffers: Optional[list[Buffer]] = None,
) -> Tab:
    """
    Create a new custom tab.

    Args:
        title: The title for the new tab
        layout: Optional layout structure for the tab
        buffers: Optional list of buffers to add to the tab

    Returns:
        The newly created Tab
    """
    ...

async def global_shortcuts() -> dict[str, Shortcut]:
    """Get a mapping of all global keyboard shortcuts."""
    ...

def set_global_shortcut(key_event: KeyEvent, shortcut: Shortcut) -> None:
    """
    Set a global keyboard shortcut.

    Args:
        key_event: The key event to bind
        shortcut: The shortcut action to execute
    """
    ...

def new_floating_window(
    window: FloatingWindow,
    *,
    id: Optional[str] = None,
    dismissible: bool = True,
    priority: DialogPriority = ...,
    timeout: Optional[float] = None,
) -> None:
    """
    Display a global floating window.

    Args:
        window: The FloatingWindow to display
        id: Optional identifier for the window
        dismissible: Whether the window can be dismissed by the user
        priority: Display priority for the window
        timeout: Optional timeout in seconds after which the window auto-dismisses
    """
    ...

def print(*args: Any, sep: Optional[str] = None, end: Optional[str] = None) -> None:
    """
    Print output to the active session's output buffer.

    Similar to Python's built-in print(), but outputs to mudpuppy's display.

    Args:
        *args: Values to print
        sep: String separator between values (default: " ")
        end: String appended after the last value (default: "\\n")
    """
    ...
