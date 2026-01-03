"""
The `pup_events` module provides decorators for event handling in Mudpuppy.

This module simplifies event handler registration by providing decorator-based
APIs for events, shortcuts, slash commands, and session setup.
"""

from __future__ import annotations

from typing import Any, Awaitable, Callable, Optional

from pup import Event, EventType, KeyEvent, Session, Tab

# Type aliases
EventHandler = Callable[[Session, Event], Awaitable[None]]
"""
A type alias for an async handler function for events.
See `event()`.
"""

ShortcutHandler = Callable[[KeyEvent, Optional[Session], Tab], Awaitable[None]]
"""
A type alias for an async handler function for shortcuts.
See `shortcut()`.
"""

SlashCommand = Callable[[str, Session], Awaitable[None]]
"""
A type alias for an async handler function for slash commands.
See `command()`.
"""

SetupHandler = Callable[[Session], Awaitable[None]]
"""
A type alias for an async handler function for new session setup.
See `setup()`.
"""

__all__ = [
    # Decorator functions
    "setup",
    "shortcut",
    "command",
    # Dynamically generated event decorators
    "config_reloaded",
    "session_closed",
    "session_connecting",
    "session_connected",
    "session_disconnected",
    "active_session_changed",
    "telnet_option_enabled",
    "telnet_option_disabled",
    "telnet_iac_command",
    "telnet_subnegotiation",
    "prompt_changed",
    "prompt_mode_changed",
    "line",
    "input_changed",
    "input_line",
    "buffer_resized",
    "tab_closed",
    "gmcp_enabled",
    "gmcp_disabled",
    "gmcp_message",
    "all",
    # General event handler decorator
    "event",
    # Type aliases defined in this module
    "EventHandler",
    "ShortcutHandler",
    "SlashCommand",
    "SetupHandler",
]

def event(
    event_type: EventType, **filters: Any
) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function as an event handler for a specific event type.

    The arguments passed to the decorated function differ based on the event
    type. Filter arguments can be provided that must match properties on the
    event for the decorated function to be called.

    Most often you don't need to use this function, and can instead use
    the more specific decorators.

    ### Decorator Args:
    * `event_type`: The type of event to handle
    * `filters`: Optional filters to apply (event properties must match)

    ## Handler args
    * `session`: a `pup.Session` associated with the event.
    * `ev`: A `pup.Event` instance matching the decorator's `pup.EventType`.

    ### Example:

    ```python
    @event(pup.EventType.Line, prompt=True)
    async def handle_prompt(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.Line)
        print(f"Prompt: {ev.line}")
    ```
    """
    ...

def shortcut(key_event: KeyEvent) -> Callable[[ShortcutHandler], ShortcutHandler]:
    """
    Register a decorated function as a keyboard shortcut handler.

    ### Decorator Args:
    * `key_event`: The `pup.KeyEvent` to bind to

    ### Handler Args:
    * `key_event`: The keyboard event that was pressed.
    * `session`: The active `pup.Session`, if any. This will be `None` for
       shortcuts executed on non-session tabs like the character list.
    * `tab`: The `pup.Tab` that was active when the keyboard shortcut was pressed.

    ### Example:

    ```python
    @shortcut(pup.KeyEvent("Ctrl-g"))
    async def my_shortcut(key_event: pup.KeyEvent, session: Optional[pup.Session], tab: pup.Tab) -> None:
        print("Ctrl-g pressed")
    ```
    """
    ...

def command(name: str) -> Callable[[SlashCommand], SlashCommand]:
    """
    Register a decorated function as a slash command handler.

    ### Decorator Args:
    * `name`: The command name (without the leading slash)

    ### Handler Args:
    * `args`: The arguments provided after the slash command `name` when invoked.
    * `session`: The active `pup.Session`.

    ### Example:

    ```python
    # Invoke in-client with '/mycommand'
    @command("mycommand")
    async def my_command(args: str, session: pup.Session) -> None:
        print(f"Command called with: {args} for {session.character}")
    ```
    """
    ...

def setup(func: SetupHandler) -> SetupHandler:
    """
    Register a decorated function as a new session setup handler.

    The decorated function will be called whenever a new session is created,
    allowing you to initialize session-specific state or handlers for a
    character.

    ### Example:

    ```python
    @setup
    async def init_session(session: pup.Session) -> None:
        print(f"New session created: {session.character}")
        # Do various other setup tasks here...
    ```
    """
    ...

# Dynamically generated event decorators (one for each EventType)
# These are convenience decorators that wrap event() with a specific EventType

def all(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for handling all event types.

    Receives all events from all sessions. The `pup.Event` can be any event subclass.

    ### Decorator args:
    * None

    ### Handler args:
    * `session`: The active `pup.Session`
    * `ev`: The `pup.Event` that occurred.

    ### Example:

    ```python
    @all()
    async def log_all_events(session: pup.Session, ev: pup.Event) -> None:
        print(f"Event: {event.type()} from {session.character}")
    ```
    """
    ...

def config_reloaded(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for configuration reload events.

    Called when the Mudpuppy configuration is reloaded because the file
    on disk (`pup.config_file()`) changed.

    If your scripts change in-memory config/settings you should re-apply
    your changes in a `config_reloaded()` handler to make sure they aren't
    undone when the config file changes on-disk.

    ### Handler args:
    * `session`: The active `pup.Session`
    * `ev`: A `pup.Event.ConfigReloaded` event instance.

    ### Example:

    ```python
    @config_reloaded()
    async def on_config_reload(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.ConfigReloaded)
        print("Configuration reloaded!")
        # Override an in-memory setting
        ev.config.settings.word_wrap = False
    ```
    """
    ...

def session_closed(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for session close events.
    Called when a session tab in the TUI is closed.

    Note, this is separate from when the session disconnects from the MUD.
    Session tabs remain open after disconnect so they can be reconnected.

    ### Handler args:
    * `session`: The `pup.Session` associated with the session tab being closed.
    * `ev`: A `pup.Event.SessionClosed` event instance.

    ### Example:

    ```python
    @session_closed()
    async def on_close(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.SessionClosed)
        print(f"Session {session.character} closed")
    ```
    """
    ...

def session_connecting(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for session connecting events.
    Called when a session begins connecting.

    ### Handler args:
    * `session`: The `pup.Session` that is connecting to a MUD.
    * `ev`: A `pup.Event.SessionConnecting` event instance.

    ### Example:

    ```python
    @session_connecting()
    async def on_connecting(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.SessionConnecting)
        print(f"Connecting to {session.character}...")
    ```
    """
    ...

def session_connected(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for session connected events.
    Called when a session successfully connects to a MUD.

    ### Handler args:
    * `session`: The `pup.Session` that connected to a MUD.
    * `ev`: A `pup.Event.SessionConnected` event instance.

    ### Example:

    ```python
    @session_connected()
    async def on_connected(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.SessionConnected)
        # Request telnet options after connection
        session.telnet().request_enable_option(31)  # NAWS
    ```
    """
    ...

def session_disconnected(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for session disconnected events.
    Called when a session disconnects from a MUD.

    ### Handler args:
    * `session`: The `pup.Session` that disconnected from a MUD.
    * `ev`: A `pup.Event.SessionDisconnected` event instance.

    ### Example:

    ```python
    @session_disconnected()
    async def on_disconnect(session: pup.Session, ev: pup.Event.SessionDisconnected) -> None:
        assert isinstance(ev, pup.Event.SessionDisconnected)
        print(f"Disconnected from {session.character}")
    ```
    """
    ...

def active_session_changed(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for active session change events.
    Called when the active session tab in the TUI changes.

    ### Handler args:
    * `session`
    * `ev`: A `pup.Event.ActiveSessionChanged` event instance.

    ### Example:

    ```python
    @active_session_changed()
    async def on_session_change(session: pup.Session, ev: pup.Event.ActiveSessionChanged) -> None:
        assert isinstance(ev, pup.Event.ActiveSessionChanged)
        if ev.changed_to:
            print(f"Switched to {ev.changed_to.character}")
    ```
    """
    ...

def telnet_option_enabled(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for telnet option enabled events.
    Called when a telnet option is enabled through telnet option negotiation with the MUD.

    ### Handler args:
    * `session`: The `pup.Session` that had the telnet option enabled.
    * `ev`: A `pup.Event.TelnetOptionEnabled` event instance.

    ### Example:

    ```python
    @telnet_option_enabled(option=31)  # NAWS
    async def on_naws_enabled(session: pup.Session, event: pup.Event.TelnetOptionEnabled) -> None:
        assert isinstance(ev, pup.Event.TelnetOptionEnabled)
        assert ev.option == 31
        print(f"NAWS option enabled for {session.character}")
    ```
    """
    ...

def telnet_option_disabled(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for telnet option disabled events.
    Called when a telnet option is disabled through telnet option negotiation with the MUD.

    ### Handler args:
    * `session`: The `pup.Session` that had the telnet option disabled.
    * `ev`: A `pup.Event.TelnetOptionDisabled` event instance.

    ### Example:

    ```python
    @telnet_option_disabled(option=42)  # CHARSET
    async def on_charset_disabled(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.TelnetOptionDisabled)
        assert ev.option == 42
        print("Charset option disabled")
    ```
    """
    ...

def telnet_iac_command(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for telnet IAC command events.
    Called when an unhandled telnet IAC command is received from the MUD.

    ### Handler args:
    * `session`: The `pup.Session` that received the telnet IAC command.
    * `ev`: A `pup.Event.TelnetIacCommand` event instance.

    Example:

    ```python
    @telnet_iac_command()
    async def on_iac_command(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.TelnetIacCommand)
        print(f"IAC command: {ev.command}")
    ```
    """
    ...

def telnet_subnegotiation(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for telnet subnegotiation events.
    Called when unhandled telnet subnegotiation data is received from the MUD.

    ### Handler args:
    * `session`: The `pup.Session` that received the telnet subnegotiation data.
    * `ev`: A `pup.Event.TelnetSubnegotiation` event instance.

    ### Example:

    ```python
    @telnet_subnegotiation(option=42)  # CHARSET
    async def on_charset_subneg(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.TelnetSubnegotiation)
        assert ev.option == 42
        if ev.data[0] == 1:  # REQUEST
            session.telnet().send_subnegotiation(42, b"\\x02UTF-8")
    ```
    """
    ...

def prompt_changed(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for prompt text change events.
    Called when the prompt text changes because a new prompt line was
    received from the MUD.

    ### Handler args:
    * `session`: The `pup.Session` that received the updated prompt.
    * `ev`: A `pup.Event.PromptChanged` event instance.

    ### Example:

    ```python
    @prompt_changed()
    async def on_prompt_change(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.PromptChanged)
        print(f"Prompt changed from '{ev.from}' to '{ev.to}'")
    ```
    """
    ...

def prompt_mode_changed(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for prompt mode change events.
    Called when the prompt mode changes, for example because the MUD negotiated
    using Telnet EOR signalling.

    ### Handler args:
    * `session`: The `pup.Session` that had the updated prompt mode.
    * `ev`: A `pup.Event.PromptModeChanged` event instance.

    ### Example:

    ```python
    @prompt_mode_changed()
    async def on_mode_change(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.PromptModeChanged)
        print(f"Prompt mode: {ev.from} -> {ev.to}")
    ```
    """
    ...

def line(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for MUD line output events.
    Called for each line received from the MUD, including prompts.
    Use a decorator filter on the `prompt` field of the `pup.Event.Line`
    event if you wish to only receive non-prompt or prompt lines.

    ### Handler args:
    * `session`: The `pup.Session` that received the output line.
    * `ev`: A `pup.Event.Line` event instance.

    ### Example:

    ```python
    @line(prompt=False)
    async def handle_prompt(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.Line)
        assert !ev.prompt
        print(f"Non-Prompt Output: {ev.line}")
    ```
    """
    ...

def input_changed(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for input change events.
    Called when the yet-to-be-sent input buffer content changes,
    e.g. because the user typed a key while on the session tab.

    Useful for implementing features like spell checking or auto-completion.

    ### Handler args:
    * `session`: The `pup.Session` that had its yet-to-be-sent input data change.
    * `ev`: A `pup.Event.InputChanged` event instance.

    ### Example:

    ```python
    @input_changed()
    async def spellcheck(session: pup.Session, ev: pup.Event.InputChanged) -> None:
        assert isinstance(ev, pup.Event.InputChanged)

        # Break up the yet-to-be-sent line
        parts = ev.line.sent.split()

        # Check spelling of each part ...

        markup: pup.Markup = ev.input.markup()

        # Add markup to the `pup.Input` with the `pup.Markup` ...
    ```
    """
    ...

def input_line(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for input line submission events.
    Called when the user hits enter, or otherwise transmits a line of input
    to the MUD.

    ### Handler args:
    * `session`: The `pup.Session` that had an input line sent.
    * `ev`: A `pup.Event.InputLine` event instance.

    ### Example:

    ```python
    @input_line()
    async def log_commands(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.InputLine)
        print(f"User sent: {ev.line.sent}")
    ```
    """
    ...

def buffer_resized(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for buffer resize events.
    Called when a buffer is resized. This can happen if the
    terminal changes sizes, or if the layout is altered.

    ### Handler args:
    * `session`: A `pup.Session` instance.
    * `ev`: A `pup.Event.BufferResized` event instance.

    ### Example:

    ```python
    @buffer_resized(name="MUD Output")
    async def on_output_resize(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.BufferResized)
        dims: pup.Dimensions = ev.to
        print(f"{session.character} main output buffer resized to {dims.width()}x{dims.height()}")
    ```
    """
    ...

def tab_closed(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for tab close events.
    Called when a `pup.Tab` is closed.

    ### Handler args:
    * `session`: A `pup.Session` instance.
    * `ev`: A `pup.Event.TabClosed` event instance.

    ### Example:

    ```python
    @tab_closed()
    async def on_tab_close(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.TabClosed)
        print(f"Tab '{ev.title}' closed")
    ```
    """
    ...

def gmcp_enabled(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for GMCP protocol enabled events.
    Called when GMCP is enabled after successful telnet negotiation with
    the MUD.

    This is a useful event handler for registering GMCP packages your
    scripts support using the `pup.Gmcp` instance.

    ### Handler args:
    * `session`: The `pup.Session` that had GMCP negotiated and enabled.
    * `ev`: A `pup.Event.GmcpEnabled` event instance.

    ### Example:

    ```python
    @gmcp_enabled()
    async def on_gmcp_enabled(session: Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.GmcpEnabled)
        # Register support for the Comm.Channel GMCP package
        session.gmcp().register("Comm.Channel")
    ```
    """
    ...

def gmcp_disabled(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for GMCP protocol disabled events.
    Called when GMCP is disabled after successful telnet negotiation with
    the MUD.

    ### Handler args:
    * `session`: The `pup.Session` that had GMCP disabled.
    * `ev`: A `pup.Event.GmcpDisabled` event instance.

    ### Example:

    ```python
    @gmcp_disabled()
    async def on_gmcp_disabled(session: pup.Session, ev: pup.Event) -> None:
        assert isinstance(ev, pup.Event.GmcpEnabled)
        print("GMCP disabled")
    ```
    """
    ...

def gmcp_message(**filters: Any) -> Callable[[EventHandler], EventHandler]:
    """
    Register a decorated function for GMCP message events.
    Called when a GMCP message is received for a session that has GMCP enabled.

    See also, `gmcp_enabled()` for registering supported packages to receive
    messages for.

    ### Handler args:
    * `session`: The `pup.Session` that received the GMCP message.
    * `ev`: A `pup.Event.GmcpMessage` event instance.

    ### Example:

    ```python
    # Assumes you've called gmcp.Register("Comm.Channel.Text") from
    # a gmcp_enabled() handler.

    @gmcp_message(package="Comm.Channel.Text")
    async def on_channel_msg(session: pup.Session, event: pup.Event) -> None:
        assert isinstance(ev, pup.Event.GmcpMessage)
        assert ev.package == "Comm.Channel.Text"
        import json
        data = json.loads(event.json)
        print(f"{data['channel']}: {data['text']}")
    ```
    """
    ...
