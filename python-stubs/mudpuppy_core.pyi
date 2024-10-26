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
    "SessionId",
    "Mud",
    "Tls",
    "MudLine",
    "InputLine",
    "Event",
    "EventType",
    "EventHandlers",
    "Trigger",
    "TriggerConfig",
    "TriggerId",
    "Alias",
    "AliasConfig",
    "AliasId",
    "TimerConfig",
    "Timer",
    "TimerId",
    "Shortcut",
    "PromptSignal",
    "PromptMode",
    "Status",
    "StreamInfo",
    "OutputItem",
    "Input",
    "EchoState",
    "LayoutNode",
    "Constraint",
    "Direction",
    "BufferConfig",
    "BufferDirection",
    "BufferId",
    "ExtraBuffer",
]

from typing import Optional, Any, Callable, Awaitable
from enum import StrEnum, auto
import datetime

class Config:
    """
    Read-only access to the Mudpuppy config.

    Accessed by calling `MudpuppyCore.config()`.
    """

    ...

type SessionId = int
"""
A `SessionInfo` identifier.

This are assigned when `EventType.NewSession` events occur. You will typically need a
`SessionId` to use as argument whenever you need to interact with a specific session
using `MudpuppyCore` methods.

You can find the `SessionInfo` associated with a `SessionId` by calling
`MudpuppyCore.session_info()`.
"""

type AliasId = int
"""
An `Alias` identifier.

These are assigned per-`SessionId` using `MudpuppyCore.new_alias()`.
"""

type TriggerId = int
"""
A `Trigger` identifier.

These are assigned per-`SessionId` using `MudpuppyCore.new_trigger()`.
"""

type TimerId = int
"""
A `Timer` identifier.

These are assigned per-`SessionId` using `MudpuppyCore.new_timer()`.
"""

type BufferId = int
"""
An `ExtraBuffer` identifier.

These are assigned per-`SessionId` using `MudpuppyCore.new_buffer()`.
"""

class SessionInfo:
    """
    Information about a session.

    Typically retrieved using `MudpuppyCore.session_info()` with a `SessionId`,
    or for all sessions, `MudpuppyCore.sessions()`.
    """

    id: SessionId
    """
    The `SessionId` for this session.
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

    Typically retrieved using `MudpuppyCore.status()` with a `SessionId`.
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

    def to_str(self) -> str:
        """
        Converts `self.raw` to a UTF8-encoded string and returns it.

        If the content is not UTF8 unknown characters will be replaced with `U+FFFD`,
        the unicode "replacement character".
        """
        ...

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

    callback: Optional[
        Callable[[SessionId, TriggerId, MudLine, list[str]], Awaitable[None]]
    ]
    """
    An optional **async** function that receives a `SessionId`, `TriggerId`, `MudLine` and list of string
    regexp groups as arguments when the trigger matches a `MudLine`.

    The `TriggerId` will be the ID of the trigger that matched the `MudLine`.
    The `MudLine` is the matched line.
    The `list[str]` is the list of string regexp groups that matched the `pattern` (if any).

    Your trigger callback function should have a signature like:

    ```python
    async def my_trigger_callback(sesh: SessionId, trigger_id: TriggerId, line: MudLine, groups: list[str]):
        ...
    ```
    """

    highlight: Optional[Callable[[MudLine, list[str]], MudLine]]
    """
    An optional **synchronous** function that receives a `MudLine` and a list of string
    regexp groups as arguments when the highlight trigger matches a `MudLine`.

    The `MudLine` returned by the callback function will **replace** the matched `MudLine`
    allowing you to (for example) add ANSI highlights colours.

    The optional `highlight` callback is invoked after the optional async `callback`
    and before the `gag` setting is applied or an `expansion` sent.

    Your highlight callback should have a signature like:

    ```python
    def my_highlight_callback(line: MudLine, groups: list[str]) -> MudLine:
        # Do some stuff here...
        return line
    ```
    """

    expansion: str
    """
    A string that will be expanded into an `InputLine` sent to the MUD whenever the
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

    def pattern(self) -> str:
        """
        Return a string representation of the `TriggerConfig` regexp pattern.
        """

class Trigger:
    """
    A `TriggerConfig` associated with a `TriggerId` after being created with `MudpuppyCore.new_trigger()`
    """

    id: TriggerId
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

    callback: Optional[
        Callable[[SessionId, AliasId, MudLine, list[str]], Awaitable[None]]
    ]
    """
    An optional **async** function that receives a `SessionId`, `AliasId`, `InputLine` and list of string
    regexp groups as arguments when the alias matches a `MudLine`.

    The `AliasId` will be the ID of the alias that matched the `MudLine`.
    The `InputLine` is the matched input line.
    The `list[str]` is the list of string regexp groups that matched the `pattern` (if any).

    Your alias callback function should have a signature like:

    ```python
    async def my_alias_callback(sesh: SessionId, alias_id: AliasId, line: InputLine, groups: list[str]):
        ...
    ```
    """

    expansion = Optional[str]
    """
    A string that will be expanded into an `InputLine` sent to the MUD whenever the
    alias matches if it is non-empty.

    This value will become the `InputLine.sent` value sent to the game, and the
    line that was matched by the alias will be set to the `InputLine.original` value.

    The sent `InputLine.scripted` property will be set to `True`.
    """

    hit_count: int
    """
    The number of times `InputLine`s have matched this `AliasConfig` since it was created.
    """

    def pattern(self) -> str:
        """
        Return a string representation of the `AliasConfig` regexp pattern.
        """

class Alias:
    """
    A `AliasConfig` associated with a `AliasId` after being created with `MudpuppyCore.new_alias()`
    """

    id: AliasId
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

class TimerConfig:
    """
    Configuration for a `Timer`.

    You can create a new `TimerConfig` by specifying a `name`, a `duration_ms`, a `callback`
    and optionally a `SessionId`:

    ```python
    timer_config = TimerConfig("Test Timer", 1000, my_timer_callback, None)
    ```

    """

    name: str
    """
    A friendly name to identify the timer.
    """

    session_id: Optional[SessionId]
    """
    The `SessionId` that the timer is associated with.
    """

    duration: datetime.timedelta
    """
    The duration  that the timer should wait before firing.
    """

    callback: Callable[[TimerId, Optional[SessionId]], Awaitable[None]]
    """
    An **async** function that receives a `TimerId` and optionally a `SessionId` when the timer fires.

    Your timer callback function should have a signature like:

    ```python
    async def my_timer_callback(timer_id: TimerId, sesh: Optional[SessionId]):
        ...
    ```
    """

class Timer:
    """
    A `TimerConfig` associated with a `TimerId` after being created with `MudpuppyCore.new_timer()`
    """

    id: TimerId
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

class LayoutNode:
    """
    A node in the layout tree.
    """

    ...

class BufferConfig:
    """
    Configuration for an `ExtraBuffer`.
    """

    ...

class ExtraBuffer:
    """
    A `BufferConfig` associated with a `BufferId` after being created with `MudpuppyCore.new_buffer()`

    An extra buffer for displaying output. Typically created by and used by scripts.
    """

    ...

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
    def name(self) -> str:
        """
        Returns the name of the program.
        """
        ...

    @staticmethod
    def version(self) -> str:
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

    async def active_session(self) -> Optional[SessionId]:
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

    async def session_info(self, session_id: SessionId) -> SessionInfo:
        """
        Returns a `SessionInfo` instance for the given session.
        """
        ...

    async def status(self, session_id: SessionId) -> Status:
        """
        Returns connection `Status` information for the given session.
        """
        ...

    async def mud_config(self, session_id: SessionId) -> Optional[Mud]:
        """
        Returns the `Mud` configuration for the given session, if it exists.
        """
        ...

    async def send_line(self, session_id: SessionId, line: str):
        """
        Sends a line of text to the given session as if it were input sent by the user.

        The input will be marked as "scripted" to differentiate it from true user input
        typed at the keyboard.
        """
        ...

    async def connect(self, session_id: SessionId):
        """
        Connects the given session if it isn't already connected.

        You can use `MudpuppyCore.status()` to determine a session's connection `Status`
        before calling `connect()`.

        A `EventType.Connection` event will be emitted with the new `Status`.
        """
        ...

    async def disconnect(self, session_id: SessionId):
        """
        Disconnects the given session if it isn't already disconnected.

        You can use `MudpuppyCore.status()` to determine a session's connection `Status`
        before calling `connect()`.

        A `EventType.Connection` event will be emitted with the new `Status`.
        """
        ...

    async def request_enable_option(self, session_id: SessionId, option: int):
        """
        Requests that the MUD server for the given session enable a telnet option.

        If the option is enabled by the server a `EventType.OptionEnabled` event will be
        emitted with the same session ID.
        """
        ...

    async def request_disable_option(self, session_id: SessionId, option: int):
        """
        Requests that the MUD server for the given session disable a telnet option.

        If the option is disabled by the server a `EventType.OptionDisabled` event will be
        emitted with the same session ID.
        """
        ...

    async def send_subnegotiation(
        self, session_id: SessionId, option: int, data: bytes
    ):
        """
        Sends a telnet subnegotiation to the given session.

        The data should be the raw bytes of the subnegotiation payload for the given option
        code.
        """
        ...

    async def new_trigger(
        self, session_id: SessionId, config: TriggerConfig, module: str
    ) -> TriggerId:
        """
        Creates a new trigger for the given session for the given `TriggerConfig`.

        Returns a `TriggerId` that can be used with `MudpuppyCore.get_trigger()`,
        `MudpuppyCore.disable_trigger()` and `MudpuppyCore.remove_trigger()`.

        The `module` str is used to associate the trigger with a specific Python module that created
        it so that if the module is reloaded, the trigger will be deleted first to avoid duplicates
        when the module re-creates it at load.
        """
        ...

    async def get_trigger(
        self, session_id: SessionId, trigger_id: TriggerId
    ) -> Optional[Trigger]:
        """
        Returns the `Trigger` for the given `TriggerId` if it exists for the provided session.

        See `MudpuppyCore.new_trigger()` for creating triggers.
        """
        ...

    async def disable_trigger(self, session_id: SessionId, trigger_id: TriggerId):
        """
        Disables the trigger with the given `TriggerId` for the given session if it
        is currently enabled.

        The trigger will no longer be evaluated when new input is received, even
        if it matches the trigger's pattern.

        You can use `MudpuppyCore.get_trigger()` to get a `Trigger` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.enable_trigger()` to
        enable the trigger again.
        """
        ...

    async def enable_trigger(self, session_id: SessionId, trigger_id: TriggerId):
        """
        Enables the trigger with the given `TriggerId` for the given session if it
        was previously disabled.

        You can use `MudpuppyCore.get_trigger()` to get a `Trigger` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.disable_trigger()` to
        disable the trigger again.
        """
        ...

    async def remove_trigger(self, session_id: SessionId, trigger_id: TriggerId):
        """
        Removes the trigger with the given `TriggerId` for the given session if it
        exists.

        The trigger will be deleted and its `TriggerId` will no longer be valid. You
        will need to recreate it with `MudpuppyCore.new_trigger()` if you want to
        restore the `TriggerConfig`.

        Prefer `MudpuppyCore.disable_trigger()` if you think you'll want the trigger
        to be used again in the future.
        """
        ...

    async def remove_module_triggers(self, session_id: SessionId, module: str):
        """
        Removes all triggers created by the given module for the given session.

        This is useful when a module is reloaded and triggers need to be recreated
        to avoid duplicates.
        """
        ...

    async def triggers(self, session_id: SessionId) -> list[Trigger]:
        """
        Returns a list of `Trigger` instances for the given session.
        """
        ...

    async def new_alias(
        self, session_id: SessionId, config: AliasConfig, module: str
    ) -> AliasId:
        """
        Creates a new `Alias` for the given session for the given `AliasConfig`.

        Returns a `AliasId` that can be used with `MudpuppyCore.get_alias()`,
        `MudpuppyCore.disable_alias()` and `MudpuppyCore.remove_alias()`.

        The `module` str is used to associate the alias with a specific Python module that created
        it so that if the module is reloaded, the alias will be deleted first to avoid duplicates
        when the module re-creates it at load.
        """
        ...

    async def get_alias(
        self, session_id: SessionId, alias_id: AliasId
    ) -> Optional[Alias]:
        """
        Returns the `Alias` for the given `AliasId` if it exists for the provided session.

        See `MudpuppyCore.new_alias()` for creating aliases.
        """
        ...

    async def disable_alias(self, session_id: SessionId, alias_id: AliasId):
        """
        Disables the alias with the given `AliasId` for the given session if it
        is currently enabled.

        The alias will no longer be evaluated when new input is received, even
        if it matches the alias's pattern.

        You can use `MudpuppyCore.get_alias()` to get a `Alias` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.enable_alias()` to
        enable the alias again.
        """
        ...

    async def enable_alias(self, session_id: SessionId, alias_id: AliasId):
        """
        Enables the alias with the given `AliasId` for the given session if it
        was previously disabled.

        You can use `MudpuppyCore.get_alias()` to get a `Alias` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.disable_alias()` to
        disable the alias again.
        """
        ...

    async def remove_alias(self, session_id: SessionId, alias_id: AliasId):
        """
        Removes the alias with the given `AliasId` for the given session if it
        exists.

        The alias will be deleted and its `AliasId` will no longer be valid. You
        will need to recreate it with `MudpuppyCore.new_alias()` if you want to
        restore the `AliasConfig`.

        Prefer `MudpuppyCore.disable_alias()` if you think you'll want the alias
        to be used again in the future.
        """
        ...

    async def remove_module_aliases(self, session_id: SessionId, module: str):
        """
        Removes all aliases created by the given module for the given session.

        This is useful when a module is reloaded and aliases need to be recreated
        to avoid duplicates.
        """
        ...

    async def aliases(self, session_id: SessionId) -> list[Alias]:
        """
        Returns a list of `Alias` instances for the given session.
        """
        ...

    async def new_timer(
        self, session_id: SessionId, config: TimerConfig, module: str
    ) -> TimerId:
        """
        Creates a new `Timer` for the given session configured with
        the given `TimerConfig`.

        Returns a `TimerId` that can be used with `MudpuppyCore.get_timer()`,
        `MudpuppyCore.stop_timer()` and `MudpuppyCore.remove_timer()`.

        The `module` str is used to associate the timer with a specific Python module that created
        it so that if the module is reloaded, the timer will be deleted first to avoid duplicates
        when the module re-creates it at load.
        """
        ...

    async def get_timer(
        self, session_id: SessionId, timer_id: TimerId
    ) -> Optional[Timer]:
        """
        Returns the `Timer` for the given `TimerId` if it exists for the provided session.

        See `MudpuppyCore.new_timer()` for creating timers.
        """
        ...

    async def stop_timer(self, session_id: SessionId, timer_id: TimerId):
        """
        Disables the timer with the given `TimerId` for the given session if it
        is currently enabled.

        The timer will no longer be evaluated when the timer interval elapses.

        You can use `MudpuppyCore.get_timer()` to get a `Timer` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.enable_timer()` to
        enable the timer again.
        """
        ...

    async def start_timer(self, session_id: SessionId, timer_id: TimerId):
        """
        Starts a timer with the given `TimerId` for the given session if it
        was previously stopped.

        You can use `MudpuppyCore.get_timer()` to get a `Timer` to determine if
        it is currently enabled or disabled. Use `MudpuppyCore.disable_timer()` to
        disable the timer again.
        """
        ...

    async def remove_timer(self, session_id: SessionId, timer_id: TimerId):
        """
        Removes the timer with the given `TimerId` for the given session if it
        exists.

        The timer will be deleted and its `TimerId` will no longer be valid. You
        will need to recreate it with `MudpuppyCore.new_timer()` if you want to
        restore the `TimerConfig`.

        Prefer `MudpuppyCore.disable_timer()` if you think you'll want the timer
        to be used again in the future.
        """
        ...

    async def remove_module_timers(self, session_id: SessionId, module: str):
        """
        Removes all timers created by the given module for the given session.

        This is useful when a module is reloaded and timers need to be recreated
        to avoid duplicates.
        """
        ...

    async def timers(self, session_id: SessionId) -> list[Timer]:
        """
        Returns a list of `Timer` instances for the given session.
        """
        ...

    async def get_input(self, session_id: SessionId) -> str:
        """
        Returns the current input line for the given session.

        This is the data that has been typed in by the user into the input area,
        but not yet transmitted to the MUD.

        Use `MudpuppyCore.set_input()` to replace this yet-to-be-sent input.
        """
        ...

    async def set_input(self, session_id: SessionId, new_input: str):
        """
        Sets the current input line for the given session.

        This is the data that has been typed in by the user into the input area,
        but not yet transmitted to the MUD.

        Use `MudpuppyCore.get_input()` to retrieve the current input.
        """
        ...

    async def add_output(self, session_id: SessionId, output: OutputItem):
        """
        Adds an `OutputItem` to the main output buffer for the given session.

        This is the primary mechanism of displaying data to the user.

        Use `MudpuppyCore.add_outputs()` if you have a `list[OutputItem]` to add.
        """
        ...

    async def dimensions(self, session_id: SessionId) -> tuple[int, int]:
        """
        Returns the width and height of the output area for the given session.

        Note that this is not the overall width/height of the window, but just the
        area just to display output from the MUD. These dimensions match the
        dimensions sent to the MUD using the Telnet
        [NAWS](https://www.rfc-editor.org/rfc/rfc1073) option if supported.

        See also `EventType.BufferResized`.
        """
        ...

    async def layout(self, session_id: SessionId) -> LayoutNode:
        """
        Returns the root `LayoutNode` for the given session.

        The layout tree describes how the output area is divided into regions
        and how each region is filled with content.

        Use `LayoutNode` methods to navigate the tree and manipulate the layout.
        """
        ...

    async def new_buffer(self, session_id: SessionId, config: BufferConfig) -> BufferId:
        """
        Creates a new `ExtraBuffer` for the given session with the given `BufferConfig`.

        Returns a `BufferId` that can be used with `MudpuppyCore.get_buffer()`,
        `MudpuppyCore.remove_buffer()`.

        Once retrieving the `ExtraBuffer` with `MudpuppyCore.get_buffer()`, you can
        use the `ExtraBuffer` methods to manipulate the buffer, add output, etc.
        """
        ...

    async def get_buffer(
        self, session_id: SessionId, buffer_id: BufferId
    ) -> Optional[ExtraBuffer]:
        """
        Returns the `ExtraBuffer` for the given `BufferId` if it exists for the provided session.

        See `MudpuppyCore.new_buffer()` for creating buffers.
        """
        ...

    async def buffers(self, session_id: SessionId) -> list[ExtraBuffer]:
        """
        Returns a list of `ExtraBuffer` instances for the given session.
        """
        ...

    async def remove_buffer(self, session_id: SessionId, buffer_id: BufferId):
        """
        Removes the buffer with the given `BufferId` for the given session if it
        exists.

        The buffer will be deleted and its `BufferId` will no longer be valid. You
        will need to recreate it with `MudpuppyCore.new_buffer()` if you want to
        restore the `BufferConfig`.
        """
        ...

    async def gmcp_enabled(self, session_id: SessionId) -> bool:
        """
        Returns `True` if negotiation has completed and GMCP is enabled for the given
        session, `False` otherwise.
        """
        ...

    async def gmcp_send(self, session_id: SessionId, module: str, json_data: str):
        """
        Sends a GMCP package to the MUD for the given session.

        The `module` is the GMCP module name and the `json` is the JSON-encoded
        data to send. You must `json.dumps()` your data to create the `json_data`
        string you provide this function.

        Use `MudpuppyCore.gmcp_enabled()` to verify GMCP is enabled for a session
        before sending GMCP messages.

        Use `MudpuppyCore.gmcp_register()` to register the `module` if required.
        """
        ...

    async def gmcp_register(self, session_id: SessionId, package: str):
        """
        Registers the given GMCP `package` with the MUD for the given session.

        This lets the MUD know you support GMCP messages for the `package`.

        Use `MudpuppyCore.gmcp_enabled()` to verify GMCP is enabled for a session
        before sending GMCP messages.

        For example, you may wish to `gmcp_register(id, "Char")` to receive `Char.*`
        package messages as events.
        """
        ...

    async def gmcp_unregister(self, session_id: SessionId, package: str):
        """
        Unregisters the given GMCP `package` with the MUD for the given session.

        This lets the MUD know you no longer want GMCP messages for the `package`.

        Use `MudpuppyCore.gmcp_enabled()` to verify GMCP is enabled for a session
        before sending GMCP messages.

        For example, you may wish to `gmcp_unregister(id, "Char")` to stop receiving `Char.*`
        package messages as events.
        """
        ...

    async def emit_event(self, custom_type: str, data: Any, id: Optional[SessionId]):
        """
        Emits a custom event with the given `custom_type` and `data` for the given session.
        If `id` is `None`, the event is emitted for all sessions.

        The event will be produced as an `EventType.Python` event.

        This can be helpful for coordinating between your Python scripts. One can
        emit a custom event and another can register a listener for it.
        """
        ...

    async def quit():
        """
        Quits the Mudpuppy client. **Terminates all sessions!**
        """
        ...

    async def reload():
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

    HistoryPrev = auto()
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

class EventType(StrEnum):
    """
    An enum describing possible `Event` types.

    You will typically specify an `EventType` when registering event handlers that will
    later be called with an `Event` instance matching that event type.
    """

    NewSession = auto()
    """
    An event emitted when a new `SessionId` is created after connecting to a `Mud`.
    """

    Connection = auto()
    """
    An event emitted when the connection for a `SessionId` changes `Status`.
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
    An event emitted for each `SessionId` after a `PythonReloaded` event.
    """

class Event:
    """
    An event emitted by Mudpuppy when something interesting happens.

    Each event has an `EventType` and you can register callbacks to
    be invoked when particular `EventType`s you are interested in occur.

    The callback will be provided an `Event` of the matching type as an
    argument.
    """

    class NewSession:
        """
        A `EventType.NewSession` event. This is produced when the user
        selects a MUD from the MUD list and an initial `SessionId` is
        assigned.
        """

        id: SessionId
        """
        The `SessionId` that was assigned for the new session.
        """

        info: SessionInfo
        """
        The `SessionInfo` describing the session. This is largely
        redundant with `id` and `mud`.
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

        id: SessionId
        """
        The `SessionId` that changed connection `Status`.
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

        id: SessionId
        """
        The `SessionId` that received the prompt.
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

        id: SessionId
        """
        The `SessionId` that received the IAC option.
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

        id: SessionId
        """
        The `SessionId` that enabled the option.
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

        id: SessionId
        """
        The `SessionId` that disabled the option.
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

        id: SessionId
        """
        The `SessionId` that received the subnegotiation.
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

        id: SessionId
        """
        The `SessionId` that had its buffer resized.
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

        id: SessionId
        """
        The `SessionId` that sent the input line.
        """

        line: InputLine
        """
        The line of input that was sent.
        """

    class Shortcut:
        """
        An `EventType.Shortcut` event. This is produced when a recognized
        keyboard shortcut is input.
        """

        id: SessionId
        """
        The `SessionId` that received the shortcut.
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

        id: SessionId
        """
        The `SessionId` that received the key press.
        """

        json: str
        """
        A JSON serialization of the key press information.

        This is a temporary hack. Sorry! In the future a proper Python class will be used.

        You probably want to `json.loads()` this string value.
        """

    class GmcpEnabled:
        """
        An `EventType.GmcpEnabled` event. This is produced when GMCP is enabled for a session
        after successfully negotiating the telnet option with the MUD server.
        """

        id: SessionId
        """
        The `SessionId` that had GMCP enabled.
        """

    class GmcpDisabled:
        """
        An `EventType.GmcpDisabled` event. This is produced when GMCP is disabled for a session.
        """

        id: SessionId
        """
        The `SessionId` that had GMCP disabled.
        """

    class GmcpMessage:
        """
        An `EventType.GmcpMessage` event. This is produced when a GMCP message is received.

        Typically this happens for `module`'s that have been registered with
        `MudpuppyCore.gmcp_register()`. To stop receiving message events for a `module`, try
        `MudpuppyCore.gmcp_unregister()`.
        """

        id: SessionId
        """
        The `SessionId` that received the GMCP message.
        """

        module: str
        """
        The GMCP module name that the message is for.
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

        id: Optional[SessionId]
        """
        The `SessionId` that emitted the custom event, or `None` if the event was emitted
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
        An `EventType.ResumeSession` event. This is produced for each `SessionId` after a
        `PythonReloaded` event.
        """

        id: SessionId
        """
        The `SessionId` that is being resumed.
        """

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

class Input:
    """
    The input area of the client window.
    """

    def value(self) -> str:
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

    def echo(self) -> EchoState:
        """
        Returns the current echo state of the input area.
        """
        ...

    def reset(self) -> Optional[str]:
        """
        Resets the input area to its default state.

        Returns the previous content (if any).
        """
        ...

    def pop(self) -> Optional[str]:
        """
        Removes and returns the input from the input area.
        """
        ...

    def set_value(self, value: str):
        """
        Sets the value of the input area, adjusting the cursor to the end.
        """
        ...

    def set_echo(self, state: EchoState):
        """
        Sets the echo state of the input area.
        """
        ...

    def set_cursor(self, pos: int):
        """
        Sets the cursor position in the input area.
        """
        ...

    def insert(self, c: chr):
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

class EventHandlers:
    """
    A collection of event handlers that will be invoked for specific registered
    `EventType`s.
    """

    def add_handler(
        self,
        event_type: EventType,
        handler: Callable[[Event], Awaitable[None]],
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

    def get_handlers(
        self, event_type: EventType
    ) -> Optional[list[Callable[[Event], Awaitable[None]]]]:
        """
        Returns a list of handlers for the given `EventType` if any are registered.
        """
        ...

    def get_handler_events(self) -> list[EventType]:
        """
        Returns a list of `EventType`s for which handlers are registered.
        """
        ...

class Constraint:
    """
    A `LayoutNode` constraint.
    """

    percentage: Optional[int]
    """
    Set when the `Constraint` is a percentage constraint.
    """

    ratio: Optional[tuple[int, int]]
    """
    Set when the `Constraint` is a ratio constraint.
    """

    length: Optional[int]
    """
    Set when the `Constraint` is a length constraint.
    """

    max: Optional[int]
    """
    Set when the `Constraint` is a max constraint.
    """

    min: Optional[int]
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
A `EventHandlers` instance for registering event handlers with the client.

It is automatically set up when Mudpuppy is running and has loaded your scripts.

You will typically want to use the `mudpuppy` decorators instead of directly
interacting with the `EventHandlers`.
"""
