"""
The `mudpuppy_core` module offers low-level access to Mudpuppy.
"""

# Defined explicitly to control rendered order in docs.
__all__ = [
    "mudpuppy_core",
    "event_handlers",
    "MudpuppyCore",
    "Config",
    "SessionInfo",
    "Mud",
    "Tls",
    "KeyEvent",
    "MouseEvent",
    "MouseEventKind",
    "KeyBindings",
    "MudLine",
    "InputLine",
    "Event",
    "EventType",
    "EventHandler",
    "EventHandlers",
    "Alias",
    "AliasCallable",
    "AliasConfig",
    "Trigger",
    "TriggerCallable",
    "HighlightCallable",
    "TriggerConfig",
    "TimerConfig",
    "Timer",
    "Shortcut",
    "PromptSignal",
    "PromptMode",
    "Status",
    "StreamInfo",
    "OutputItem",
    "Output",
    "Input",
    "EchoState",
    "LayoutNode",
    "Constraint",
    "Direction",
    "BufferConfig",
    "BufferDirection",
    "ExtraBuffer",
    "Gauge",
    "Button",
    "ButtonCallable",
]

from typing import Optional, Any, Callable, Awaitable, Tuple
from enum import StrEnum, auto
import datetime

class Tls(StrEnum):
    """
    Describes whether/how TLS should be used when connecting to a `Mud`.
    """

    Disabled = auto()
    """
    TLS is not used. Plain (insecure) Telnet should be used.
    """

    Enabled = auto()
    """
    TLS should be used and the server certificate chain verified.
    """

    VerifySkipped = auto()
    """
    TLS should be used, but certificate errors should be ignored.

    This is generally unsafe but may be required if the server is misconfigured
    or if you're using self-signed test certificates.
    """

class Mud:
    """
    Information about a MUD and its configuration.
    """

    name: str
    """
    Name of the MUD.

    Used as the label for the session tab, and for listing the MUD on the connection
    screen.

    This is the identifier you will use with decorators like `mudpuppy.trigger` for the
    `mud_name` parameter.
    """

    host: str
    """
    Host address of the MUD.

    This is typically a domain name like `"dunemud.net"` or an IP address like `8.8.8.8` or
    `2607:f8b0:400b:803::200e`.

    The `port` number is specified separately and not included here.
    """

    port: int
    """
    Port number of the MUD.

    This varies by game, and may change based on whether you're using TLS or not.
    """

    tls: Tls
    """
    Describes the TLS configuration for the MUD.
    """

    command_separator: Optional[str]
    """
    An optional command separator to use when sending multiple commands in a single line.
    """

class KeyEvent:
    """
    A key press event.
    """
    def code(self) -> str:
        """
        Returns the key code for the event.

        Example: "a", "q", "f10"
        """

    def modifiers(self) -> list[str]:
        """
        Return a list of key modifiers active for the key code.

        Example: "ctrl", "shift", "alt"
        """

class MouseEventKind(StrEnum):
    """
    An enum describing possible `MouseEvent` types.
    """

    LeftButtonDown = auto()
    """
    The left mouse button was pressed.
    """

    RightButtonDown = auto()
    """
    The right mouse button was pressed.
    """

    MiddleButtonDown = auto()
    """
    The middle mouse button was pressed.
    """

    Moved = auto()
    """
    The mouse was moved.
    """

    ScrollDown = auto()
    """
    The mouse wheel was scrolled down.
    """

    ScrollUp = auto()
    """
    The mouse wheel was scrolled up.
    """

    ScrollLeft = auto()
    """
    The mouse wheel was scrolled left.
    """

    ScrollRight = auto()
    """
    The mouse wheel was scrolled right.
    """

class MouseEvent:
    """
    A mouse event.
    """

    kind: MouseEventKind
    """
    The `MouseEventKind` of mouse event that occurred.
    """

    column: int
    """
    The terminal column where the event occurred.
    """

    row: int
    """
    The terminal row where the event occurred.
    """

    def modifiers(self) -> list[str]:
        """
        Return a list of key modifiers active for the mouse event.

        Example: "ctrl", "shift", "alt"
        """

class Shortcut(StrEnum):
    """
    A recognized keyboard shortcut.
    """

    Quit = auto()
    """
    A shortcut to quit the client
    """

    TabNext = auto()
    """
    A shortcut to change to the next tab.
    """

    TabPrev = auto()
    """
    A shortcut to change to the previous tab.
    """

    TabClose = auto()
    """
    A shortcut to close the current tab.
    """

    TabSwapLeft = auto()
    """
    A shortcut to swap the current tab with the tab to the left.
    """

    TabSwapRight = auto()
    """
    A shortcut to swap the current tab with the tab to the right.
    """

    MudListNext = auto()
    """
    A shortcut to select the next MUD from the MUD list.
    """

    MudListPrev = auto()
    """
    A shortcut to select the previous MUD from the MUD list.
    """

    MudListConnect = auto()
    """
    A shortcut to connect to the selected MUD from the MUD list.
    """

    ToggleLineWrap = auto()
    """
    A shortcut to toggle line wrapping in the output area.
    """

    ToggleInputEcho = auto()
    """
    A shortcut to toggle whether `InputLine`s are displayed in the output buffer.
    """

    HistoryNext = auto()
    """
    A shortcut to navigate to the next line in the input history.
    """

    HistoryPrevious = auto()
    """
    A shortcut to navigate to the previous line in the input history.
    """

    ScrollUp = auto()
    """
    A shortcut to scroll the output buffer up.
    """

    ScrollDown = auto()
    """
    A shortcut to scroll the output buffer down.
    """

    ScrollTop = auto()
    """
    A shortcut to scroll the output buffer to the top.
    """

    ScrollBottom = auto()
    """
    A shortcut to scroll the output buffer to the bottom.
    """

class KeyBindings:
    """
    Read-only Key binding configuration.
    See `Config` for more information.
    """

    def modes(self) -> list[str]:
        """
        Returns a list of all the key binding input modes.
        """

    def bindings(self, mode: Optional[str]) -> list[tuple[KeyEvent, Shortcut]]:
        """
        Returns a dictionary of all the key bindings for the given mode. If no mode
        is specified a default of "mudsession" is used.

        Raises an exception if mode is not a known input mode.
        Use `modes` for a list of all available modes.
        """

    def shortcut(self, event: KeyEvent, mode: Optional[str]) -> Optional[Shortcut]:
        """
        Returns the `Shortcut` for the given `KeyEvent` in the given mode, or `None`
        if no binding exists in the mode for the `KeyEvent`. If no mode is specified
        a default of "mudsession" is used.

        Raises an exception if mode is not a known input mode.
        Use `modes` for a list of all available modes.
        """

class Config:
    """
    Read-only access to the Mudpuppy config.

    Accessed by calling `MudpuppyCore.config()`.
    """

    def lookup_mud(self, mud_name: str) -> Optional[Mud]:
        """
        Return the `Mud` configuration for the given `mud_name`, or `None` if no
        configuration exists for the given `mud_name`.
        """

    def must_lookup_mud(self, mud_name: str) -> Mud:
        """
        Return the `Mud` configuration for the given `mud_name`, or raise an
        exception if no configuration exists for the given `mud_name`.
        """

    def keybindings(self) -> KeyBindings:
        """
        Return the `KeyBindings` configuration.
        """

class SessionInfo:
    """
    Information about a session.

    Typically retrieved using `MudpuppyCore.session_info()` with an `int` session ID,
    or for all sessions, `MudpuppyCore.sessions()`.
    """

    id: int
    """
    The session ID identifier for this session.
    """

    mud_name: str
    """
    The name of the MUD that the session is connected to.
    """

class StreamInfo:
    """
    Information about a connection stream.
    """

    class Tcp:
        """
        A normal Telnet TCP stream without any encryption or authentication.
        """

        ip: str
        """
        The IP address the stream is connected to, in string format.

        This may be an IPv4 or IPv6 address.
        """

        port: int
        """
        The port the stream is connected to.
        """

    class Tls:
        """
        A TLS encrypted stream.
        """

        ip: str
        """
        The IP address the stream is connected to, in string format.

        This may be an IPv4 or IPv6 address.
        """

        port: int
        """
        The port the stream is connected to.
        """

        protocol: str
        """
        The TLS protocol version in use, in string format.
        """

        ciphersuite: str
        """
        The TLS ciphersuite in use, in string format.
        """

        verify_skipped: bool
        """
        Whether the TLS certificate verification was skipped.

        When `verify_skipped` is `True`, the stream was configured to ignore
        any certificate errors (dangerous!).
        """

class Status:
    """
    Connection status information.

    Typically retrieved using `MudpuppyCore.status()` with an `int` session ID.
    """

    class Disconnected:
        """
        The session is not connected.
        """

        ...

    class Connecting:
        """
        The session is in the process of connecting.
        """

        ...

    class Connected:
        """
        The session is connected and `info` holds the `StreamInfo` describing
        the server the stream is connected to.
        """

        info: StreamInfo
        """
        The `StreamInfo` for the connected stream.

        This will be a `StreamInfo.Tcp` or `StreamInfo.Tls` instance depending
        on the `Mud` that was used to create the stream.
        """

class MudLine:
    """
    A line received from a MUD.
    """

    raw: bytes
    """
    The raw bytes received from the game.

    For regular lines this will be the content without the trailing newline indicator
    (`\\r\\n`) that terminated the line.

    If ANSI colours are used, the control codes will be present in `raw` unaltered.
    """

    prompt: bool
    """
    Whether or not the line was considered a prompt.

    Prompt lines are typically terminated explicitly, or flushed from the connection
    buffer after a certain timeout without receiving a normal `\\r\\n` line ending.
    """

    gag: bool
    """
    Whether the line was gagged by a trigger (e.g. not displayed in output).
    """

    def __init__(self, value: bytes):
        """
        Construct a new `MudLine` with the given `value` bytes.

        """
        ...

    def to_str(self) -> str:
        """
        Converts `self.raw` to a UTF8-encoded string and returns it.

        If the content is not UTF8 unknown characters will be replaced with `U+FFFD`,
        the unicode "replacement character".
        """
        ...

    def stripped(self) -> str:
        """
        Returns `self.raw` after converting to UTF-8 using `to_str()` and then
        stripping ANSI control sequences.
        """

    def set(self, new: str):
        """
        Sets the `MudLine`'s `raw` value to the UTF-8 bytes of the string `new`.
        """

type TriggerCallable = Callable[[int, int, MudLine, list[str]], Awaitable[None]]
"""
An async function that is called when output sent from a MUD matches a trigger pattern.
Typically assigned to a `TriggerConfig`'s `callback` property, alternatively see
`mudpuppy.trigger()` for a simple `@trigger()` decorator.

The handler is called with:

* the `int` session ID of the session that received the output
* the `int` trigger ID of the `mudpuppy_core.Trigger` that matched.
* the `str` output that matched the trigger pattern, and
* a `list[str]` of captured groups from the trigger pattern (if any).

Example:
```python
from mudpuppy_core import mudpuppy_core

async def my_trigger_handler(
    session_id: int,
    trigger_id: int,
    line: str,
    _groups: list[str]
):
    trigger: Trigger = await mudpuppy_core.get_trigger(session_id, trigger_id)
    print(f"trigger {trigger.config.name} has matched output: {line}")
    print(f"this trigger has matched output {trigger.config.hits} times so far")
```
"""

type HighlightCallable = Callable[[MudLine, list[str]], MudLine]
"""
A **non-async** function that is called when a line of output from the MUD matches a
highlight pattern. Typically assigned to a `TriggerConfig`'s `highlight` property.
Alternatively see `mudpuppy.highlight()` for a simple `@highlight()` decorator.

The handler is called with:
* a `MudLine` object representing the line of output from the MUD
* a `list[str]` of captured groups from the highlight pattern (if any)

It **must** return a `MudLine` to display. This can be the same line
object passed in, or a new line object.

Unlike `AliasCallable`, `TriggerCallable`, and most other callables
`mudpuppy_core` uses this callable is **not** async. This is because the handler is expected to mutate
the `MudLine` object in place to apply the desired highlighting. This
also means you can not `await` other `mudpuppy_core` functions from within a highlight
and should prefer using `TriggerCallable()` handlers for those tasks.

For example, you could use `MudLine.set_line()` to mutate the provided
`line` to add ANSI colours using `cformat`:

```python
from cformat import cformat

# Note: **not async**!
def example_highlight_callable(line: MudLine, groups):
    assert len(groups) == 1
    new_line = line.__str__().replace(
        groups[0], cformat(f"<bold><cyan>{groups[0]}<reset>")
    )
    line.set(new_line)
    return line
```
"""

class TriggerConfig:
    """
    Configuration for a `Trigger`.

    You can create a new `TriggerConfig` by specifying a regexp `pattern` and a `name`:

    ```python
    trigger_config = TriggerConfig(r".*Hello.*", "hello trigger")
    trigger_config.gag = True
    ```
    """

    name: str
    """
    A friendly name to identify the trigger.
    """

    strip_ansi: bool
    """
    Whether or not ANSI colours should be stripped **before** the trigger `pattern` is matched.

    Typically you will want this to be `True` unless you want to write a `pattern` that matches
    on specific colours.
    """

    prompt: bool
    """
    Whether or not the `MudLine` is required to have `MudLine.prompt` be equal to `True` in
    addition to the `pattern` matching for the trigger to fire.

    Set this to `True` if you only want the trigger to match lines that generate a `EventType.Prompt`
    event.
    """

    gag: bool
    """
    Whether or not `MudLine`s matched by this trigger should be gagged (e.g. not displayed).

    Set this to `True` to suppress (gag) matched lines.
    """

    callback: Optional[TriggerCallable] = None
    """
    An optional **async** `TriggerCallable` to invoke when the trigger matches.
    """

    highlight: Optional[HighlightCallable] = None
    """
    An optional **synchronous** `HighlightCallable` to invoke when the `pattern` matches
    to provide a new `MudLine` to display.

    The `MudLine` returned by the callback function will **replace** the matched `MudLine`
    allowing you to (for example) add ANSI highlights colours.
    """

    expansion: Optional[str] = None
    """
    An optional string that will be expanded into an `InputLine` sent to the MUD whenever the
    trigger matches if it is non-empty.

    The sent `MudLine` will have `MudLine.scripted` set to `True` to differentiate
    it from human input.

    The expansion will be used after both the optional `callback` and `highlight` functions
    have been called.
    """

    hit_count: int
    """
    The number of times `OutputLine`s have matched this `Trigger` since it was created.
    """

    def __init__(
        self,
        pattern: str,
        name: str,
        *,
        strip_ansi: bool = False,
        prompt: bool = False,
        gag: bool = False,
        callback: Optional[TriggerCallable] = None,
        highlight: Optional[HighlightCallable] = None,
        expansion: Optional[str] = None,
    ):
        """
        Create a new `TriggerConfig` with a `pattern` and a `name`.

        Optionally you may specify `strip_ansi`, `prompt`, `gag`, `callback`, `highlight`, and `expansion`.
        """
        ...

    def pattern(self) -> str:
        """
        Return a string representation of the `TriggerConfig` regexp pattern.
        """

class Trigger:
    """
    A `TriggerConfig` associated with a `int` trigger ID after being created with `MudpuppyCore.new_trigger()`
    """

    id: int
    """
    The `Trigger`'s ID.
    """

    enabled: bool
    """
    Whether the `Trigger` is currently enabled.

    Mutate using `MudpuppyCore.enable_trigger()`.
    """

    module: bool
    """
    The module that created the `Trigger`.

    Used in association with `MudpuppyCore.remove_module_triggers()`.
    """

    config: TriggerConfig
    """
    The `TriggerConfig` for the `Trigger`.
    """

type AliasCallable = Callable[[int, int, str, list[str]], Awaitable[None]]
"""
An async function that is called when input sent to a MUD matches an alias pattern.
Typically you will assign an `AliasCallable` to the `callback` property of an `AliasConfig`.
Alternatively, see `mudpuppy.alias()` for a simple `@alias()` decorator.

The handler is called with:

* the `int` session ID of the session that received the input
* the `int` alias ID of the `Alias` that matched.
* the `str` input that matched the alias pattern, and
* a `list[str]` of captured groups from the alias pattern (if any).

Example:
```python
from mudpuppy_core import mudpuppy_core, Alias

async def my_alias_handler(session_id: int, alias_id: int, line: str, _groups: list[str]):
    alias: Alias = await mudpuppy_core.get_alias(session_id, alias_id)
    print(f"alias {alias.config.name} has matched input: {line}")
    print(f"this alias has matched input {alias.config.hits} times so far")
```
"""

class AliasConfig:
    """
    Configuration for an `Alias`.

    You can create a new `AliasConfig` by specifying a regexp `pattern` and a `name`:

    ```python
    alias_config = AliasConfig(r"^hello$", "hello trigger")
    alias_config.expansion = "say HELLO!"
    ```
    """

    name: str
    """
    A friendly name to identify the alias.
    """

    callback: Optional[AliasCallable] = None
    """
    An optional **async** `AliasCallable` to invoke when the alias matches.
    """

    hit_count: int
    """
    The number of times `InputLine`s have matched this `AliasConfig` since it was created.
    """

    def __init__(
        self,
        pattern: str,
        name: str,
        callback: Optional[AliasCallable] = None,
        expansion: Optional[str] = None,
    ):
        """
        Create a new `AliasConfig` with a `pattern` and a `name`.

        You can optionally provide a `callback` and an `expansion` string.
        """
        ...

    def pattern(self) -> str:
        """
        Return a string representation of the `AliasConfig` regexp pattern.
        """
        ...

    @property
    def expansion(self) -> Optional[str]:
        """
        An optional string that will be expanded into an `InputLine` sent to the MUD whenever the
        alias matches if it is non-empty.

        This value will become the `InputLine.sent` value sent to the game, and the
        line that was matched by the alias will be set to the `InputLine.original` value.

        The sent `InputLine.scripted` property will be set to `True`.
        """
        ...

    @expansion.setter
    def expansion(self, value: Optional[str]):
        """
        Set the `expansion` property.
        """
        ...

class Alias:
    """
    A `AliasConfig` associated with a `int` alias ID after being created with `MudpuppyCore.new_alias()`
    """

    id: int
    """
    The `Alias`'s ID.
    """

    enabled: bool
    """
    Whether the `Alias` is currently enabled.

    Mutate using `MudpuppyCore.enable_alias()` and `MudpuppyCore.disable_alias()`.
    """

    module: str
    """
    The module that created the `Alias`.

    Used in association with `MudpuppyCore.remove_module_aliases()`.
    """

    config: AliasConfig
    """
    The `AliasConfig` for the `Alias`.
    """

type TimerCallable = Callable[[int, Optional[int]], Awaitable[None]]

class TimerConfig:
    """
    Configuration for a `Timer`.

    You can create a new `TimerConfig` by specifying a `name`, a `duration_ms`, a `callback`
    and optionally an `int` session ID:

    ```python
    timer_config = TimerConfig("Test Timer", 1000, my_timer_callback, None)
    ```

    """

    name: str
    """
    A friendly name to identify the timer.
    """

    duration: datetime.timedelta
    """
    The duration  that the timer should wait before firing.
    """

    callback: TimerCallable
    """
    An **async** function that receives an `int` timer ID and optionally an `int` session ID when the timer fires.

    Your timer callback function should have a signature like:

    ```python
    async def my_timer_callback(timer_id: int, sesh: Optional[int]):
        ...
    ```
    """

    def __init__(
        self,
        name: str,
        duration_ms: int,
        callback: TimerCallable,
        session: Optional[int] = None,
    ):
        """
        Create a new `TimerConfig` with a `name` that will be run every `duration_ms`
        milliseconds, invoking `callback`. The timer may optionally be associated
        with an `int` session ID.
        """
        ...

    @property
    def session_id(self) -> Optional[int]:
        """
        An optional `int` session ID that the timer is associated with. Can be both read and set.
        """
        ...

    @session_id.setter
    def session_id(self, id: Optional[int]):
        """
        Set the `int` session ID.
        """
        ...

    @property
    def max_ticks(self) -> Optional[int]:
        """
        An optional maximum number of times the `callback` should be invoked before
        the timer is automatically removed. Can be both read and set.
        """
        ...

    @max_ticks.setter
    def max_ticks(self, ticks: Optional[int]):
        """
        Set the `max_ticks`
        """
        ...

class Timer:
    """
    A `TimerConfig` associated with an `int` timer ID after being created with `MudpuppyCore.new_timer()`
    """

    id: int
    """
    The `Timer`'s ID.
    """

    running: bool
    """
    Whether the `Timer` is currently running.

    Mutate using `MudpuppyCore.start_timer()` and `MudpuppyCore.stop_timer()`.
    """

    module: str
    """
    The module that created the `Timer`.

    Used in association with `MudpuppyCore.remove_module_timers()`.
    """

    config: TimerConfig
    """
    The `TimerConfig` for the `Timer`.
    """

class EchoState(StrEnum):
    """
    The echo state for an `InputLine`
    """

    Enabled = auto()
    """
    Echo was enabled and the `InputLine` was displayed normally.
    """

    Password = auto()
    """
    Telnet echo was disabled because the `InputLine` was a password.

    It should be displayed masked (e.g. `*****`)
    """

class InputLine:
    """
    A line of input that was transmitted to the MUD.
    """

    sent: str
    """
    The string that was transmitted to the MUD.
    """

    original: str
    """
    In the event that an `Alias` changed the input line, this will be the
    original input that the `Alias` matched. The `sent` value will be what
    the `Alias` expanded to.
    """

    echo: EchoState
    """
    The `EchoState` for the `InputLine`. This indicates if the line was masked
    (e.g. because it was a password entry) or not.
    """

    scripted: bool
    """
    The `scripted` property is `True` when the input wasn't sent by a human
    entering it with the keyboard but was instead sent programmatically by a
    script.
    """

    def __init__(
        self,
        line: str,
        echo: bool,
        scripted: bool,
    ):
        """
        Create a new `InputLine` for `line`.

        If `echo` is `False`, the line will be masked when displayed.

        If `scripted` is `True`, the `line` should be considered generated by a script and not a human.
        """
        ...

    def clone_with_original(
        self,
    ) -> "InputLine":
        """
        Create a new `InputLine` by cloning the original, but replacing `sent` with `original`.

        This is primarily used when translating a `InputLine` from history into one for use
        with `MudpuppyCore.set_input()`.
        """
        ...

    def split(
        self,
        separator: str,
    ) -> list["InputLine"]:
        """
        Split the `InputLine` by `separator`, returning an `InputLine` for each part.
        """
        ...

    def empty(self) -> bool:
        """
        Returns true if the input line is empty.
        """
        ...

class OutputItem:
    """
    An item to be displayed in the output area of a session.
    """

    class Mud:
        """
        A line of text from the MUD.
        """

        line: MudLine
        """
        The `MudLine` to be displayed.
        """

    class Input:
        """
        A line of text from the user.
        """

        line: InputLine
        """
        The `InputLine` to be displayed.
        """

    class Prompt:
        """
        A prompt line.
        """

        line: MudLine
        """
        The prompt `MudLine` to be displayed.
        """

    class HeldPrompt:
        """
        A prompt line that has been held at the bottom of the MUD buffer
        for consistent display as output scrolls.
        """

        line: MudLine
        """
        The held prompt `MudLine` to be displayed.
        """

    class ConnectionEvent:
        """
        A connection event to be displayed in the output buffer.
        """

        status: Status
        """
        The updated `Status` of the connection.
        """

    class CommandResult:
        """
        A command result to be displayed in the output buffer.
        """

        message: str
        """
        The message produced from running the command.
        """

        error: bool
        """
        Whether the command succeeded or produced an error.
        """

    class PreviousSession:
        """
        A message loaded from a previous session log.
        """

        line: MudLine
        """
        The `MudLine` from the previous session.
        """

    class Debug:
        """
        A debug message to be displayed in the output buffer.
        """

        line: str
        """
        The debug line.
        """

    @staticmethod
    def mud(line: MudLine) -> "OutputItem":
        """
        Construct a `Mud` `OutputItem` with the given `MudLine`.
        """
        ...

    @staticmethod
    def command_result(msg: str) -> "OutputItem":
        """
        Construct a `CommandResult` `OutputItem` with the given `msg`.

        The `CommandResult.error` will be `False`. For a failed command
        result output item, use `failed_command_result()`.
        """

    @staticmethod
    def failed_command_result(msg: str) -> "OutputItem":
        """
        Construct a failed `CommandResult` `OutputItem` with the given `msg`.

        The `CommandResult.error` will be `True`.
        """

    @staticmethod
    def previous_session(line: MudLine) -> "OutputItem":
        """
        Construct a `PreviousSession` `OutputItem` with the given `line`.
        """

    @staticmethod
    def debug(line: str) -> "OutputItem":
        """
        Construct a `Debug` `OutputItem` with the given `line`.
        """

class Output:
    """
    A collection of `OutputItem` instances displayed in an `ExtraBuffer`.
    """

    def len(self) -> int:
        """
        Returns the number of `OutputItem` instances in the collection.
        """
        ...

    def is_empty(self) -> bool:
        """
        Returns `True` if the collection is empty, `False` otherwise.
        """
        ...

    def push(self, item: OutputItem):
        """
        Appends an `OutputItem` to the collection.

        The `BufferConfig.direction` will determine whether items added at the end
        of the collection are rendered first, or last.
        """

    def set(self, items: list[OutputItem]):
        """
        Sets the collection of `OutputItem` instances to `items`.

        Replaces the existing content with the `items` list.
        """
        ...

class Constraint:
    """
    A `LayoutNode` constraint.
    """

    percentage: Optional[int] = None
    """
    Set when the `Constraint` is a percentage constraint.
    """

    ratio: Optional[tuple[int, int]] = None
    """
    Set when the `Constraint` is a ratio constraint.
    """

    length: Optional[int] = None
    """
    Set when the `Constraint` is a length constraint.
    """

    max: Optional[int] = None
    """
    Set when the `Constraint` is a max constraint.
    """

    min: Optional[int] = None
    """
    Set when the `Constraint` is a min constraint.
    """

    @staticmethod
    def with_percentage(percentage: int) -> "Constraint":
        """
        Creates a new `Constraint` with a percentage constraint.
        """
        ...

    def set_percentage(self, percentage: int):
        """
        Sets the percentage constraint.
        """
        ...

    @staticmethod
    def with_ratio(ratio: tuple[int, int]) -> "Constraint":
        """
        Creates a new `Constraint` with a ratio constraint.
        """
        ...

    def set_ratio(self, ratio: tuple[int, int]):
        """
        Sets the ratio constraint.
        """
        ...

    @staticmethod
    def with_length(length: int) -> "Constraint":
        """
        Creates a new `Constraint` with a length constraint.
        """
        ...

    def set_length(self, length: int):
        """
        Sets the length constraint.
        """
        ...

    @staticmethod
    def with_max(max: int) -> "Constraint":
        """
        Creates a new `Constraint` with a max constraint.
        """
        ...

    def set_max(self, max: int):
        """
        Sets the max constraint.
        """
        ...

    @staticmethod
    def with_min(min: int) -> "Constraint":
        """
        Creates a new `Constraint` with a min constraint.
        """
        ...

    def set_min(self, min: int):
        """
        Sets the min constraint.
        """
        ...

    def set_from(self, other: "Constraint"):
        """
        Sets the values of this `Constraint` from another `Constraint`.
        """
        ...

class Direction(StrEnum):
    """
    A direction to use when creating new layout nodes.
    """

    Horizontal = auto()
    """
    Create new sections in the horizontal direction.
    """

    Vertical = auto()
    """
    Create new sections in the vertical direction.
    """

class LayoutNode:
    """
    Each layout node describes a section in the layout tree.
    """

    def __init__(self, name: str):
        """
        Creates a new `LayoutNode` with the given `name`.
        """
        ...

    @property
    def name(self) -> str:
        """
        Name of the section. Can be both read and set.
        """
        ...

    @name.setter
    def name(self, name: str):
        """
        Update the name of the section.
        """
        ...

    @property
    def direction(self) -> Direction:
        """
        Direction the sub-sections are laid out. Can be both read and set.
        """
        ...

    @direction.setter
    def direction(self, dir: Direction):
        """
        Update the direction of the section.
        """
        ...

    @property
    def margin(self) -> int:
        """
        Margin between sub-sections. Can be both read and set.
        """
        ...

    @margin.setter
    def margin(self, margin: int):
        """
        Update the margin between sub-sections.
        """
        ...

    @property
    def sections(self) -> list[Tuple[Constraint, "LayoutNode"]]:
        """
        The list of sub-sections (if any). Can be both read and set.

        Each sub-section is described as a `Tuple` holding a `Constraint` and a `LayoutNode`.
        """
        ...

    @sections.setter
    def sections(self, sections: list[Tuple[Constraint, "LayoutNode"]]):
        """
        Update the list of sub-sections.
        """
        ...

    def add_section(self, section: "LayoutNode", constraint: Constraint):
        """
        Adds a `LayoutNode` as a child section of this node, with size described by
        the given `constraint`.
        """
        ...

    def find_section(self, name: str) -> tuple[Constraint, "LayoutNode"]:
        """
        Returns the `Constraint` and `LayoutNode` for the section with the given `name`.

        Raises an exception if `name` is not a known section name under the `LayoutNode`.
        """
        ...

    def all_layouts(self) -> dict[str, "LayoutNode"]:
        """
        Returns a dictionary of all the `LayoutNode` instances in the layout tree,
        keyed by their section name.
        """
        ...

class BufferDirection(StrEnum):
    """
    Describes what direction an `ExtraBuffer` should render its contents.
    """

    BottomToTop = auto()
    """
    The default direction, and the way the standard MUD output buffer works.

    Newer items should be rendered first, at the bottom of the buffer. Older
    items will be rendered afterwards towards the top of the buffer.
    """

    TopToBottom = auto()
    """
    Older items should be rendered first, at the top of the buffer. Newer items
    will be rendered afterwards, towards the bottom of the buffer.
    """

class BufferConfig:
    """
    Configuration for an `ExtraBuffer`.

    See `MudpuppyCore.new_buffer()` for more information.
    """

    layout_name: str
    """
    The name of the layout section that the buffer should be displayed in.

    See `layout.LayoutManager` for more information.
    """
    ...

    line_wrap: bool
    """
    Whether the content in the `ExtraBuffer` should be line-wrapped.
    """

    border_top: bool
    """
    Whether the top border of the `ExtraBuffer` should be displayed.
    """

    border_bottom: bool
    """
    Whether the bottom border of the `ExtraBuffer` should be displayed.
    """

    border_left: bool
    """
    Whether the left border of the `ExtraBuffer` should be displayed.
    """

    border_right: bool
    """
    Whether the right border of the `ExtraBuffer` should be displayed.
    """

    direction: BufferDirection
    """
    The `BufferDirection` that the content of the `ExtraBuffer` should be displayed in.
    """

    output: Output
    """
    The `Output` collection of `OutputItem`s to be displayed in the `ExtraBuffer`.
    """

    scroll_pos: int
    """
    The scroll position of the `ExtraBuffer`.
    """

    max_scroll: int
    """
    The maximum `scroll_pos` of the `ExtraBuffer`.
    """

    def __init__(self, layout_name: str):
        """
        Create a new `BufferConfig` with the given `layout_name`.
        """
        ...

class ExtraBuffer:
    """
    A `BufferConfig` associated with an `int` buffer ID after being created with `MudpuppyCore.new_buffer()`

    An extra buffer for displaying output. Typically created by and used by scripts.
    """

    id: int
    """
    The `int` buffer ID assigned to the buffer.
    """

    config: BufferConfig
    """
    The `BufferConfig` of the `ExtraBuffer`
    """

class Gauge:
    """
    A gauge widget that can be created with `MudpuppyCore.new_gauge()`.
    """

    id: int
    """
    The read-only `int` ID assigned to the gauge.
    """

    layout_name: str
    """
    The layout name where the `Gauge` should be drawn.

    Can be both read and set.
    """

    value: float
    """
    The gauge's current value.

    Can be both read and set.
    """

    max: float
    """
    The gauge's maximum value.

    Can be both read and set.
    """

    title: str
    """
    The title label for the gauge.
    """

    def set_colour(self, r: int, g: int, b: int):
        """
        Set the colour of the gauge to the RGB values provided.
        """
        ...

    def set_color(self, r: int, g: int, b: int):
        """
        Set the color of the gauge to the RGB values provided.
        """
        ...

type ButtonCallable = Callable[[int, int], Awaitable[None]]
"""
An async function that is called when a button is clicked.

Typically assigned to a `Button`'s `callback` property.

The callable is called with:

* the `int` session ID of the session that owns the button that was clicked.
* the `int` button ID of the button that was clicked.
"""

class Button:
    """
    A button widget that can be created with `MudpuppyCore.new_button()`.
    """

    id: int
    """
    The read-only `int` ID assigned to the button.
    """

    layout_name: str
    """
    The layout name where the `Button` should be drawn.

    Can be both read and set.
    """

    label: str
    """
    The label for the button.
    """

    callback: ButtonCallable

class Input:
    """
    The input area of the client window.
    """

    def value(self) -> InputLine:
        """
        Returns the current value of the input area.
        """
        ...

    def cursor(self) -> int:
        """
        Returns the current cursor position in the input area.
        """
        ...

    def visual_cursor(self) -> int:
        """
        Returns the visual cursor position in the input area.
        """
        ...

    def visual_scroll(self, width: int) -> int:
        """
        Returns the visual scroll position in the input area.
        """
        ...

    def telnet_echo(self) -> EchoState:
        """
        Returns the current telnet echo state of the input area.
        """
        ...

    def reset(self):
        """
        Resets the input area to its default state.
        """
        ...

    def pop(self) -> Optional[InputLine]:
        """
        Removes and returns the input from the input area.
        """
        ...

    def set_value(self, value: InputLine):
        """
        Sets the value of the input area, adjusting the cursor to the end.
        """
        ...

    def set_telnet_echo(self, state: EchoState):
        """
        Sets the telnet echo state of the input area.
        """
        ...

    def set_cursor(self, pos: int):
        """
        Sets the cursor position in the input area.
        """
        ...

    def insert(self, c: str):
        """
        Inserts a character at the cursor position.
        """
        ...

    def delete_prev(self):
        """
        Deletes the character before the cursor.
        """
        ...

    def delete_next(self):
        """
        Deletes the character after the cursor.
        """
        ...

    def delete_word_left(self):
        """
        Deletes the word to the left of the cursor.
        """
        ...

    def delete_word_right(self):
        """
        Deletes the word to the right of the cursor.
        """
        ...

    def delete_to_end(self):
        """
        Deletes from the cursor to the end of the input.
        """
        ...

    def cursor_left(self):
        """
        Moves the cursor left.
        """
        ...

    def cursor_right(self):
        """
        Moves the cursor right.
        """
        ...

    def cursor_word_left(self):
        """
        Moves the cursor to the left word boundary.
        """
        ...

    def cursor_word_right(self):
        """
        Moves the cursor to the right word boundary.
        """
        ...

    def cursor_start(self):
        """
        Moves the cursor to the start of the input.
        """
        ...

    def cursor_end(self):
        """
        Moves the cursor to the end of the input.
        """
        ...

    def drop_index(self, index: int):
        """
        Drops the character at the given index.
        """
        ...

    def add_markup(self, index: int, ansi_markup: str):
        """
        Adds the provided ansi_markup as a decoration to the input area at the given index.
        """

    def remove_markup(self, index: int):
        """
        Removes the ansi_markup decoration from the given index.
        """

    def clear_markup(self):
        """
        Clears all ansi_markup decorations from the input area.
        """

    def markup(self) -> dict[int, str]:
        """
        Returns a dictionary of ansi_markup decorations in the input area.
        """

    def decorated_value(self) -> str:
        """
        Returns the input area's value with the ANSI markup interposed at the correct positions.
        """

class MudpuppyCore:
    def config(self) -> Config:
        """
        Returns a `Config` instance.

        Note that when the configuration changes old `Config` instances
        are not automatically updated. You should use a
        `EventType.ConfigReloaded` event handler to respond to updates
        to `Config`.
        """
        ...

    @staticmethod
    def config_dir() -> str:
        """
        Returns the path to the Mudpuppy configuration directory.

        This is the place where you should place `.py` scripts to be
        automatically loaded, and the default location for the `Config`
        file.
        """
        ...

    @staticmethod
    def data_dir() -> str:
        """
        Returns the path to the Mudpuppy data directory.

        This is the place where Mudpuppy writes its logfile.
        """
        ...

    @staticmethod
    def name() -> str:
        """
        Returns the name of the program.
        """
        ...

    @staticmethod
    def version() -> str:
        """
        Returns the version of the program.
        """
        ...

    async def print(self, *args, sep: Optional[str] = None, end: Optional[str] = None):
        """
        Outputs each line of the rendered arguments as debug output items in
        the currently active mudpuppy session (if any).

        The arguments and behaviour match that of `builtins.print`.

        For more control over output, use `MudpuppyCore.add_output()` instead.
        """

    async def active_session_id(self) -> Optional[int]:
        """
        Returns the ID of the currently active session, or `None` if no session
        is active.
        """
        ...

    async def sessions(self) -> list[SessionInfo]:
        """
        Returns a list of `SessionInfo` instances for all sessions.
        """
        ...

    async def session_info(self, session_id: int) -> SessionInfo:
        """
        Returns a `SessionInfo` instance for the given session ID.
        """
        ...

    async def status(self, session_id: int) -> Status:
        """
        Returns connection `Status` information for the given session ID.
        """
        ...

    async def mud_config(self, session_id: int) -> Optional[Mud]:
        """
        Returns the `Mud` configuration for the given session ID, if it exists.
        """
        ...

    async def send_line(self, session_id: int, line: str):
        """
        Sends a line of text to the given session ID as if it were input sent by the user.

        The input will be marked as "scripted" to differentiate it from true user input
        typed at the keyboard.

        [Command splitting](https://mudpuppy-rs.github.io/mudpuppy/user-guide/input.html#command-splitting)
        works the same as for normal user input.

        Unlike true user input, aliases are **not** evaluated for `send_line()`
        input. This also means it isn't possible to send slash
        [command](https://mudpuppy-rs.github.io/mudpuppy/user-guide/commands.html)
        input in this manner.

        Prefer using `MudpuppyCore.send_lines()` for sending multiple lines.

        Example:

        ```python
        from mudpuppy_core import mudpuppy_core
        session_id = ...
        mudpuppy_core.send_line(session_id, "hello") # Sends 'hello'
        mudpuppy_core.send_line(session_id, "hello;;wave") # Sends 'hello' and then 'wave'
        ```
        """
        ...

    async def send_lines(self, session_id: int, lines: list[str]):
        """
        Sends a list of lines of text to the given session ID as if they were input sent by the user.

        The input will be marked as "scripted" to differentiate it from true user input
        typed at the keyboard.

        Prefer using `MudpuppyCore.send_line()` for sending a single line.
        """
        ...

    async def connect(self, session_id: int):
        """
        Connects the given session ID if it isn't already connected.

        You can use `MudpuppyCore.status()` to determine a session's connection `Status`
        before calling `connect()`.

        A `EventType.Connection` event will be emitted with the new `Status`.
        """
        ...

    async def disconnect(self, session_id: int):
        """
        Disconnects the given session ID if it isn't already disconnected.

        You can use `MudpuppyCore.status()` to determine a session's connection `Status`
        before calling `connect()`.

        A `EventType.Connection` event will be emitted with the new `Status`.
        """
        ...

    async def request_enable_option(self, session_id: int, option: int):
        """
        Requests that the MUD server enable a telnet option for the given session ID.

        If the option is enabled by the server a `EventType.OptionEnabled` event will be
        emitted with the same session ID.
        """
        ...

    async def request_disable_option(self, session_id: int, option: int):
        """
        Requests that the MUD server disable a telnet option for the given session ID.

        If the option is disabled by the server a `EventType.OptionDisabled` event will be
        emitted with the same session ID.
        """
        ...

    async def send_subnegotiation(self, session_id: int, option: int, data: bytes):
        """
        Sends a telnet subnegotiation to the given session.

        The data should be the raw bytes of the subnegotiation payload for the given option
        code.
        """
        ...

    async def new_trigger(
        self, session_id: int, config: TriggerConfig, module: str
    ) -> int:
        """
        Creates a new trigger for the given session ID for the given `TriggerConfig`.

        Returns an `int` trigger ID that can be used with `MudpuppyCore.get_trigger()`,
        `MudpuppyCore.disable_trigger()` and `MudpuppyCore.remove_trigger()`.

        The `module` str is used to associate the trigger with a specific Python module that created
        it so that if the module is reloaded, the trigger will be deleted first to avoid duplicates
        when the module re-creates it at load.
        """
        ...

    async def get_trigger(self, session_id: int, trigger_id: int) -> Optional[Trigger]:
        """
        Returns the `Trigger` for the given trigger ID if it exists for the provided session ID.

        See `MudpuppyCore.new_trigger()` for creating triggers.
        """
        ...

    async def disable_trigger(self, session_id: int, trigger_id: int):
        """
        Disables the trigger with the given trigger ID for the given session ID if it
        is currently enabled.

        The trigger will no longer be evaluated when new input is received, even
        if it matches the trigger's pattern.

        You can use `MudpuppyCore.get_trigger()` to get a `Trigger` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.enable_trigger()` to
        enable the trigger again.
        """
        ...

    async def enable_trigger(self, session_id: int, trigger_id: int):
        """
        Enables the trigger with the given trigger ID for the given session ID if it
        was previously disabled.

        You can use `MudpuppyCore.get_trigger()` to get a `Trigger` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.disable_trigger()` to
        disable the trigger again.
        """
        ...

    async def remove_trigger(self, session_id: int, trigger_id: int):
        """
        Removes the trigger with the given trigger ID for the given session ID if it
        exists.

        The trigger will be deleted and its trigger ID will no longer be valid. You
        will need to recreate it with `MudpuppyCore.new_trigger()` if you want to
        restore the `TriggerConfig`.

        Prefer `MudpuppyCore.disable_trigger()` if you think you'll want the trigger
        to be used again in the future.
        """
        ...

    async def remove_module_triggers(self, session_id: int, module: str):
        """
        Removes all triggers created by the given module for the given session ID.

        This is useful when a module is reloaded and triggers need to be recreated
        to avoid duplicates.
        """
        ...

    async def triggers(self, session_id: int) -> list[Trigger]:
        """
        Returns a list of `Trigger` instances for the given session ID.
        """
        ...

    async def new_alias(self, session_id: int, config: AliasConfig, module: str) -> int:
        """
        Creates a new `Alias` for the given session ID for the given `AliasConfig`.

        Returns an `int` alias ID that can be used with `MudpuppyCore.get_alias()`,
        `MudpuppyCore.disable_alias()` and `MudpuppyCore.remove_alias()`.

        The `module` str is used to associate the alias with a specific Python module that created
        it so that if the module is reloaded, the alias will be deleted first to avoid duplicates
        when the module re-creates it at load.
        """
        ...

    async def get_alias(self, session_id: int, alias_id: int) -> Optional[Alias]:
        """
        Returns the `Alias` for the given alias ID if it exists for the provided session ID.

        See `MudpuppyCore.new_alias()` for creating aliases.
        """
        ...

    async def disable_alias(self, session_id: int, alias_id: int):
        """
        Disables the alias with the given alias ID for the given session if it
        is currently enabled.

        The alias will no longer be evaluated when new input is received, even
        if it matches the alias's pattern.

        You can use `MudpuppyCore.get_alias()` to get a `Alias` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.enable_alias()` to
        enable the alias again.
        """
        ...

    async def enable_alias(self, session_id: int, alias_id: int):
        """
        Enables the alias with the given alias ID for the given session ID if it
        was previously disabled.

        You can use `MudpuppyCore.get_alias()` to get a `Alias` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.disable_alias()` to
        disable the alias again.
        """
        ...

    async def remove_alias(self, session_id: int, alias_id: int):
        """
        Removes the alias with the given alias ID for the given session ID if it
        exists.

        The alias will be deleted and its alias ID will no longer be valid. You
        will need to recreate it with `MudpuppyCore.new_alias()` if you want to
        restore the `AliasConfig`.

        Prefer `MudpuppyCore.disable_alias()` if you think you'll want the alias
        to be used again in the future.
        """
        ...

    async def remove_module_aliases(self, session_id: int, module: str):
        """
        Removes all aliases created by the given module for the given session ID.

        This is useful when a module is reloaded and aliases need to be recreated
        to avoid duplicates.
        """
        ...

    async def aliases(self, session_id: int) -> list[Alias]:
        """
        Returns a list of `Alias` instances for the given session ID.
        """
        ...

    async def new_timer(self, config: TimerConfig, module: str) -> int:
        """
        Creates a new `Timer` configured with the given `TimerConfig`.

        Returns an `int` timer ID that can be used with `MudpuppyCore.get_timer()`,
        `MudpuppyCore.stop_timer()` and `MudpuppyCore.remove_timer()`.

        The `module` str is used to associate the timer with a specific Python module that created
        it so that if the module is reloaded, the timer will be deleted first to avoid duplicates
        when the module re-creates it at load.
        """
        ...

    async def get_timer(self, timer_id: int) -> Optional[Timer]:
        """
        Returns the `Timer` for the given timer ID if it exists.

        See `MudpuppyCore.new_timer()` for creating timers.
        """
        ...

    async def stop_timer(self, timer_id: int):
        """
        Disables the timer with the given timer ID if it is currently enabled.

        The timer will no longer be evaluated when the timer interval elapses.

        You can use `MudpuppyCore.get_timer()` to get a `Timer` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.enable_timer()` to
        enable the timer again.
        """
        ...

    async def start_timer(self, timer_id: int):
        """
        Starts a timer with the given timer ID if it was previously stopped.

        You can use `MudpuppyCore.get_timer()` to get a `Timer` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.disable_timer()` to
        disable the timer again.
        """
        ...

    async def remove_timer(self, timer_id: int):
        """
        Removes the timer with the given timer ID if it exists.

        The timer will be deleted and its timer ID will no longer be valid. You
        will need to recreate it with `MudpuppyCore.new_timer()` if you want to
        restore the `TimerConfig`.

        Prefer `MudpuppyCore.disable_timer()` if you think you'll want the timer
        to be used again in the future.
        """
        ...

    async def remove_module_timers(self, module: str):
        """
        Removes all timers created by the given module.

        This is useful when a module is reloaded and timers need to be recreated
        to avoid duplicates.
        """
        ...

    async def timers(self) -> list[Timer]:
        """
        Returns a list of `Timer` instances.
        """
        ...

    async def input(self, session_id: int) -> Input:
        """
        Returns the `Input` for the given session ID.

        The `Input` provides access to queued input typed by the user and has
        functions to query/edit/replace that input.
        """
        ...

    async def add_output(self, session_id: int, output: OutputItem):
        """
        Adds an `OutputItem` to the main output buffer for the given session ID.

        This is the primary mechanism of displaying data to the user.

        Use `MudpuppyCore.add_outputs()` if you have a `list[OutputItem]` to add.
        """
        ...

    async def add_outputs(self, session_id: int, outputs: list[OutputItem]):
        """
        Adds a list of `OutputItem` instances to the main output buffer for the given session ID.

        USe `MudpuppyCore.add_output()` if you only have one `OutputItem` to add.
        """
        ...

    async def dimensions(self, session_id: int) -> tuple[int, int]:
        """
        Returns the width and height of the output area for the given session ID.

        Note that this is not the overall width/height of the window, but just the
        area just to display output from the MUD. These dimensions match the
        dimensions sent to the MUD using the Telnet
        [NAWS](https://www.rfc-editor.org/rfc/rfc1073) option if supported.

        See also `EventType.BufferResized`.
        """
        ...

    async def layout(self, session_id: int) -> LayoutNode:
        """
        Returns the root `LayoutNode` for the given session ID.

        The layout tree describes how the output area is divided into regions
        and how each region is filled with content.

        Use `LayoutNode` methods to navigate the tree and manipulate the layout.
        """
        ...

    async def new_buffer(self, session_id: int, config: BufferConfig) -> int:
        """
        Creates a new `ExtraBuffer` for the given session ID with the given `BufferConfig`.

        Returns an `int` buffer ID that can be used with `MudpuppyCore.get_buffer()`,
        `MudpuppyCore.remove_buffer()`.

        Once retrieving the `ExtraBuffer` with `MudpuppyCore.get_buffer()`, you can
        use the `ExtraBuffer` methods to manipulate the buffer, add output, etc.
        """
        ...

    async def get_buffer(
        self, session_id: int, buffer_id: int
    ) -> Optional[ExtraBuffer]:
        """
        Returns the `ExtraBuffer` for the given buffer ID  if it exists for the provided session ID.

        See `MudpuppyCore.new_buffer()` for creating buffers.
        """
        ...

    async def buffers(self, session_id: int) -> list[ExtraBuffer]:
        """
        Returns a list of `ExtraBuffer` instances for the given session ID.
        """
        ...

    async def remove_buffer(self, session_id: int, buffer_id: int):
        """
        Removes the buffer with the given buffer ID for the given session ID if it
        exists.

        The buffer will be deleted and its buffer ID will no longer be valid. You
        will need to recreate it with `MudpuppyCore.new_buffer()` if you want to
        restore the `BufferConfig`.
        """
        ...

    async def new_gauge(
        self,
        session_id: int,
        *,
        title: Optional[str] = None,
        layout_name: Optional[str] = None,
        value: Optional[float] = None,
        max: Optional[float] = None,
        rgb: Optional[Tuple[int, int, int]] = None,
    ) -> Gauge:
        """
        Creates a new `Gauge` based on the provided arguments, for the given `session_id`.

        Returns the created `Gauge` instance. You can read/write values of this instance
        to customize the gauge.
        """
        ...

    async def new_button(
        self,
        session_id: int,
        callback: ButtonCallable,
        *,
        label: Optional[str] = None,
        layout_name: Optional[str] = None,
    ) -> Button:
        """
        Creates a new `Button` based on the provided arguments, for the given `session_id`.
        The `ButtonCallable` will be invoked when the button is clicked.

        Returns the created `Button` instance. You can read/write values of this instance
        to customize the button.
        """
        ...

    async def gmcp_enabled(self, session_id: int) -> bool:
        """
        Returns `True` if negotiation has completed and GMCP is enabled for the given
        session ID, `False` otherwise.
        """
        ...

    async def gmcp_send(self, session_id: int, module: str, json_data: str):
        """
        Sends a GMCP package to the MUD for the given session ID.

        The `module` is the GMCP module name and the `json` is the JSON-encoded
        data to send. You must `json.dumps()` your data to create the `json_data`
        string you provide this function.

        Use `MudpuppyCore.gmcp_enabled()` to verify GMCP is enabled for a session
        before sending GMCP messages.

        Use `MudpuppyCore.gmcp_register()` to register the `module` if required.
        """
        ...

    async def gmcp_register(self, session_id: int, package: str):
        """
        Registers the given GMCP `package` with the MUD for the given session ID.

        This lets the MUD know you support GMCP messages for the `package`.

        Use `MudpuppyCore.gmcp_enabled()` to verify GMCP is enabled for a session
        before sending GMCP messages.

        For example, you may wish to `gmcp_register(id, "Char")` to receive `Char.*`
        package messages as events.
        """
        ...

    async def gmcp_unregister(self, session_id: int, package: str):
        """
        Unregisters the given GMCP `package` with the MUD for the given session ID.

        This lets the MUD know you no longer want GMCP messages for the `package`.

        Use `MudpuppyCore.gmcp_enabled()` to verify GMCP is enabled for a session
        before sending GMCP messages.

        For example, you may wish to `gmcp_unregister(id, "Char")` to stop receiving `Char.*`
        package messages as events.
        """
        ...

    async def emit_event(self, custom_type: str, data: Any, session_id: Optional[int]):
        """
        Emits a custom event with the given `custom_type` and `data` for the given session ID.
        If `id` is `None`, the event is emitted for all sessions.

        The event will be produced as an `EventType.Python` event.

        This can be helpful for coordinating between your Python scripts. One can
        emit a custom event and another can register a listener for it.
        """
        ...

    async def quit(self):
        """
        Quits the Mudpuppy client. **Terminates all sessions!**
        """
        ...

    async def reload(self):
        """
        Reloads all Python scripts.

        Before the reload occurs already loaded scripts will have their `on_reload()`
        function called (if it exists) before the reload happens. Similarly,
        `MudpuppyCore.remove_module_aliases()`,`MudpuppyCore.remove_module_triggers()`, and
        `MudpuppyCore.remove_module_timers()` will be called for each of the reloaded modules.

        Remember that events that already occurred (e.g. `EventType.NewSession`) will not
        be re-emitted. Your scripts should be written to pick up where they left off from
        before the reload without requiring extra events beyond `EventType.PythonReloaded`.
        """
        ...

class EventType(StrEnum):
    """
    An enum describing possible `Event` types.

    You will typically specify an `EventType` when registering event handlers that will
    later be called with an `Event` instance matching that event type.
    """

    NewSession = auto()
    """
    An event emitted when a new session ID is created after connecting to a `Mud`.
    """

    Connection = auto()
    """
    An event emitted when the connection for a session ID changes `Status`.
    """

    Prompt = auto()
    """
    An event emitted when a prompt is received.
    """

    ConfigReloaded = auto()
    """
    An event emitted when the `Config` has been reloaded.

    This happens when the config file on disk has been edited, or a setting was changed.
    """

    PythonReloaded = auto()
    """
    An event emitted when Python code has been reloaded.

    This is emitted after `MudpuppyCore.reload()` has been called, and the reload process
    completed.
    """

    Iac = auto()
    """
    An event emitted when a Telnet IAC option was received.
    """

    OptionEnabled = auto()
    """
    An event emitted when a Telnet option was enabled. Typically in response
    to a `MudpuppyCore.request_enable_option()` call.
    """

    OptionDisabled = auto()
    """
    An event emitted when a Telnet option was disabled. Typically in response
    to a `MudpuppyCore.request_disable_option()` call.
    """

    Subnegotiation = auto()
    """
    An event emitted when a Telnet subnegotiation was received.
    """

    BufferResized = auto()
    """
    An event emitted when the MUD output buffer is resized.
    Typically this happens when the overall window has been resized, or layout element
    changes have occurred.
    """

    InputLine = auto()
    """
    An event emitted after a line of input was sent to the MUD.
    """

    Shortcut = auto()
    """
    An event emitted when a recognized keyboard shortcut was input.
    """

    KeyPress = auto()
    """
    An event emitted when a keyboard key was pressed.
    """

    Mouse = auto()
    """
    An event emitted when there is mouse activity.
    """

    Python = auto()
    """
    A custom event was emitted by a Python script.

    See `MudpuppyCore.emit_event()`.
    """

    GmcpEnabled = auto()
    """
    An event emitted when GMCP is enabled for a session.

    See also `MudpuppyCore.gmcp_enabled()`.
    """

    GmcpDisabled = auto()
    """
    An event emitted when GMCP is disabled for a session.

    See also `MudpuppyCore.gmcp_enabled()`.
    """

    GmcpMessage = auto()
    """
    An event emitted when a GMCP message is received.
    """

    ResumeSession = auto()
    """
    An event emitted for each session ID after a `PythonReloaded` event.
    """

class Event:
    """
    An event emitted by Mudpuppy when something interesting happens.

    Each event has an `EventType` and you can register callbacks to
    be invoked when particular `EventType`s you are interested in occur.

    The callback will be provided an `Event` of the matching type as an
    argument.
    """

    def session_id(self) -> Optional[int]:
        """
        Returns the session ID associated with the event, if any.

        Returns `None` for global events.
        """
        ...

    class NewSession:
        """
        A `EventType.NewSession` event. This is produced when the user
        selects a MUD from the MUD list and an initial session ID is
        assigned.
        """

        id: int
        """
        The session ID that was assigned for the new session.
        """

        info: SessionInfo
        """
        The `SessionInfo` describing the session.
        """

        mud: Mud
        """
        The `Mud` that the session connected to.
        """

    class Connection:
        """
        An `EvenType.Connection` event. This is produced when the
        `Status` of the session's connection changes.
        """

        id: int
        """
        The session ID that changed connection `Status`.
        """

        status: Status
        """
        The new `Status` of the connection.
        """

    class Prompt:
        """
        An `EventType.Prompt` event. This is produced when a prompt
        is received from the MUD.
        """

        id: int
        """
        The session ID that received the prompt.
        """

        prompt: MudLine
        """
        The prompt `MudLine` that was received.

        The `MudLine.prompt` value will always be `true` for `MudLine`s received
        as part of a `Prompt` event.
        """

    class Iac:
        """
        An `EventType.Iac` event. This is produced when a Telnet IAC
        option is received.
        """

        id: int
        """
        The session ID that received the IAC option.
        """

        command: int
        """
        The telnet IAC command code that was received.
        """

    class OptionEnabled:
        """
        An `EventType.OptionEnabled` event. This is produced when a
        Telnet option is enabled. Typically in response to a
        `MudpuppyCore.request_enable_option()` call.
        """

        id: int
        """
        The session ID that enabled the option.
        """

        option: int
        """
        The Telnet option code that was enabled.
        """

    class OptionDisabled:
        """
        An `EventType.OptionDisabled` event. This is produced when a
        Telnet option is disabled. Typically in response to a
        `MudpuppyCore.request_disable_option()` call.
        """

        id: int
        """
        The session ID that disabled the option.
        """

        option: int
        """
        The Telnet option code that was disabled.
        """

    class Subnegotiation:
        """
        An `EventType.Subnegotiation` event. This is produced when a
        Telnet subnegotiation is received.
        """

        id: int
        """
        The session ID that received the subnegotiation.
        """

        option: int
        """
        The Telnet option code that was negotiated.
        """

        data: bytes
        """
        The raw bytes of the subnegotiation payload.

        This is everything between the IAC SB and the IAC SE telnet protocol
        markers.
        """

    class BufferResized:
        """
        An `EventType.BufferResized` event. This is produced when the MUD
        output buffer is resized.

        The dimensions included in the event describe the new size of the MUD
        output area (e.g. not the entire Mudpuppy window - just the area where
        MUD output is displayed).
        """

        id: int
        """
        The session ID that had its buffer resized.
        """

        dimensions: tuple[int, int]
        """
        The new width and height of the MUD output area.
        """

    class InputLine:
        """
        An `EventType.InputLine` event. This is produced after a line of input
        is sent to the MUD.
        """

        id: int
        """
        The session ID that sent the input line.
        """

        input: InputLine
        """
        The line of input that was sent.
        """

    class Shortcut:
        """
        An `EventType.Shortcut` event. This is produced when a recognized
        keyboard shortcut is input.
        """

        id: int
        """
        The session ID that received the shortcut.
        """

        shortcut: Shortcut
        """
        The shortcut that was recognized.
        """

    class KeyPress:
        """
        An `EventType.KeyPress` event. This is produced when a keyboard key
        is pressed.
        """

        id: int
        """
        The session ID that received the key press.
        """

        key: KeyEvent
        """
        The `KeyEvent` describing the key that was pressed.
        """

    class Mouse:
        """
        An `EventType.Mouse` event. This is produced when there is mouse activity and the Mudpuppy config has
        `mouse_enabled` set to `true`.
        """

        id: int
        """
        The session ID that received the mouse activity.
        """

        event: MouseEvent
        """
        The `MouseEvent` describing the mouse activity.
        """

    class GmcpEnabled:
        """
        An `EventType.GmcpEnabled` event. This is produced when GMCP is enabled for a session
        after successfully negotiating the telnet option with the MUD server.
        """

        id: int
        """
        The session ID that had GMCP enabled.
        """

    class GmcpDisabled:
        """
        An `EventType.GmcpDisabled` event. This is produced when GMCP is disabled for a session.
        """

        id: int
        """
        The session ID that had GMCP disabled.
        """

    class GmcpMessage:
        """
        An `EventType.GmcpMessage` event. This is produced when a GMCP message is received.

        Typically this happens for `module`'s that have been registered with
        `MudpuppyCore.gmcp_register()`. To stop receiving message events for a `module`, try
        `MudpuppyCore.gmcp_unregister()`.
        """

        id: int
        """
        The session ID that received the GMCP message.
        """

        package: str
        """
        The GMCP package name that the message is for.
        """

        json: str
        """
        The JSON-encoded data for the GMCP message.
        """

    class Python:
        """
        An `EventType.Python` event. This is produced when a custom event is emitted
        with `MudpuppyCore.emit_event()`.
        """

        id: Optional[int]
        """
        The session ID that emitted the custom event, or `None` if the event was emitted
        for all sessions.
        """

        custom_type: str
        """
        The custom event type that was emitted.
        """

        data: Any
        """
        The data that was emitted with the event.
        """

    class ConfigReloaded:
        """
        An `EventType.ConfigReloaded` event. This is produced when the `Config` has been reloaded.

        This happens when the config file on disk has been edited, or a setting was changed.

        You should call `MudpuppyCore.config()` after receiving this event to get a copy of
        the latest `Config`.
        """

    class PythonReloaded:
        """
        An `EventType.PythonReloaded` event. This is produced when Python code has been reloaded.

        This is emitted after `MudpuppyCore.reload()` has been called, and the reload process
        completed.
        """

    class ResumeSession:
        """
        An `EventType.ResumeSession` event. This is produced for each session ID after a
        `PythonReloaded` event.
        """

        id: int
        """
        The session ID that is being resumed.
        """

class PromptSignal(StrEnum):
    """
    Describes a possible way of signalling the a partial line is a prompt
    and not part of a line waiting for the other part with the line terminator
    to be received.
    """

    EndOfRecord = auto()
    """
    Prompts are signalled by the telnet end of record (EOR) IAC signal.
    """

    GoAhead = auto()
    """
    Prompts are signalled by the telnet go ahead (GA) IAC signal.
    """

    ...

class PromptMode:
    """
    The mode the MUD is using for handling prompt lines.
    """

    class Unsignalled:
        """
        The MUD does not explicitly signal prompt lines.

        Instead, after `timeout` if we find we have received a partial line
        without a normal `\\r\\n` line terminator, we flush the partial line
        as an assumed unterminated prompt.

        If another bit of data is received before the `timeout`, we reset the
        timeout process.
        """

        timeout: datetime.timedelta
        """
        The timeout after which a partial line is assumed to be a prompt.
        """

    class Signalled:
        """
        The MUD explicitly signals prompt lines using a specified `PromptSignal`.
        """

        signal: PromptSignal
        """
        The `PromptSignal` used to indicate prompt lines.
        """

type EventHandler = Callable[[Event], Awaitable[None]]
"""
An async function that handles a `mudpuppy_core.Event` object as its sole argument.

Example:
```python
async def my_event_handler(event: mudpuppy_core.Event):
    print(f"my_event_handler received event {event}")
```
"""

class EventHandlers:
    """
    A collection of event handlers that will be invoked for specific registered
    `EventType`s.
    """

    def add_handler(
        self,
        event_type: EventType,
        handler: EventHandler,
        module: str,
    ):
        """
        Adds a new event handler for the given `EventType`.

        The async `handler` will be invoked when an event of the given type is emitted.

        The `module` string is used to associate the handler with a specific Python
        module so that when the module is reloaded, the handler can be removed to
        avoid duplicates.

        The `handler` should have a signature like:
        ```python
        async def handler(event: Event):
            ...
        ```
        """
        ...

    def get_handlers(self, event_type: EventType) -> Optional[list[EventHandler]]:
        """
        Returns a list of handlers for the given `EventType` if any are registered.
        """
        ...

    def get_handler_events(self) -> list[EventType]:
        """
        Returns a list of `EventType`s for which handlers are registered.
        """
        ...

mudpuppy_core: MudpuppyCore
"""
A `MudpuppyCore` instance for interacting with the client.

It is automatically set up when Mudpuppy is running and has loaded
your scripts.

You will typically want to call functions on the `mudpuppy_core.mudpuppy_core`
instance to interact with the client. For example,

```python
from mudpuppy_core import mudpuppy_core
version = mudpuppy_core.version()
print(f"running {version}")
```
"""

event_handlers: EventHandlers
"""
An `EventHandlers` instance for registering `EventHandler` instances
with the client.

It is automatically set up when Mudpuppy is running and has loaded your scripts.

You will typically want to use the `mudpuppy` decorators (e.g. `mudpuppy.on_event()`)
instead of directly interacting with the `EventHandlers`.

To manually register an `EventHandler`, you can use `EventHandlers.add_handler()`:

```python
from mudpuppy_core import mudpuppy_core, event_handlers, Event, EventType

async def on_gmcp_enabled(event: Event):
    print(f"GMCP enabled for session {event.id}")
    await mudpuppy_core.gmcp_register(event.id, "Char")

event_handlers.add_handler(EventType.GmcpEnabled, on_gmcp_enabled, __name__)
```
"""
