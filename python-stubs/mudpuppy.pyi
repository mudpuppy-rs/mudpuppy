"""
The `mudpuppy` module offers helpful decorators built on top of `mudpuppy_core`.

These decorators can be used to set up handlers for events, GMCP messages, and to create
triggers, aliases and timers automatically at session creation time.

In most cases each global decorator has a variant that allows you to accomplish the
same task, but for one or more specific `mudpuppy_core.Mud` names. For example,
`on_event()` and `on_mud_event()`, or `on_connected()` and `on_mud_connected()`.
"""

# Defined explicitly to control rendered order in docs.
__all__ = [
    "on_event",
    "on_mud_event",
    "on_new_session",
    "on_mud_new_session",
    "on_new_session_or_reload",
    "on_mud_new_session_or_reload",
    "on_connected",
    "on_mud_connected",
    "on_disconnected",
    "on_mud_disconnected",
    "GmcpHandler",
    "on_gmcp",
    "alias",
    "trigger",
    "highlight",
    "TimerCallable",
    "timer",
    "alias_max_hits",
    "trigger_max_hits",
    "unload_handlers",
]

from typing import Callable, Awaitable, Union, Optional, Any
import mudpuppy_core

type GmcpHandler = Callable[[mudpuppy_core.SessionId, Any], Awaitable[None]]
"""
An async function that receives a `mudpuppy_core.SessionId` and GMCP data as its arguments.

Example:
```python
async def my_gmcp_handler(session_id: mudpuppy_core.SessionId, data: Any):
    print(f"my_gmcp_handler session {session_id} got GMCP data {data}")
```
"""

def on_event(
    event_type: Union[mudpuppy_core.EventType, list[mudpuppy_core.EventType]],
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    Decorator to register an async `mudpuppy_core.EventHandler` function as an event handler
    for one or more event types.

    The decorated function will be called with a `mudpuppy_core.Event` object when the
    specified `mudpuppy_core.EventType`'s are received.

    The decorated function **must** be async or an error will be produced by
    the decorator.

    If `module` is `None`, then `EventHandler.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    If you want to register an event handler only for a specific set of MUDs, use
    `on_mud_event()` instead. If you require more control over the event handler
    registration process you may prefer to manually use `mudpuppy_core.event_handlers`.

    Example:
    ```python
    @on_event(mudpuppy_core.EventType.Subnegotiation)
    async def telnet_subneg_receive(event: mudpuppy_core.Event):
        assert isinstance(event, mudpuppy_core.Event.Subnegotiation)
        if event.option != 42:
            return
        print(f"telnet CHARSET subneg. for session {event.id}: {event.data.hex()}")
    ````
    """
    ...

def on_mud_event(
    mud_name: Union[str, list[str]],
    event_type: Union[mudpuppy_core.EventType, list[mudpuppy_core.EventType]],
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    Equivalent to `on_event()` but will only be invoked when the event
    occurs for a session with `mudpuppy_core.SessionInfo.mud_name` equal to one
    of the specified `mud_name`'s.

    Example:
    ```python
    @on_mud_event(["Test (TLS)", "Test (Telnet)"], mudpuppy_core.EventType.Prompt)
    async def prompt_handler(event: mudpuppy_core.Event):
        assert isinstance(event, mudpuppy_core.Event.Prompt)
        print(f"Test MUD received prompt: {str(event.prompt)}")
    ```
    """
    ...

def on_gmcp(
    package: str, module: Optional[str] = None
) -> Callable[[GmcpHandler], GmcpHandler]:
    """
    Decorator to register a `GmcpHandler` as a handler for a specific GMCP package.
    You will typically need to register intent for that GMCP package using
    `mudpuppy_core.MudpuppyCore.gmcp_register()` after checking GMCP has been negotiated with
    `mudpuppy_core.MudpuppyCore.gmcp_enabled()`, or in response to a
    `mudpuppy_core.EventType.GmcpEnabled`
    event.

    The decorated function will be called with the `mudpuppy_core.SessionId` and the
    GMCP message data whenever a `mudpuppy_core.EventType.GmcpMessage` event for the
    specified package is received.

    If `module` is `None`, then `GmcpHandler.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    Example:
    ```python
    # Make sure the "Char" package is registered
    @on_event(mudpuppy_core.EventType.GmcpEnabled):
    async def gmcp_ready(event: mudpuppy_core.Event):
        await mudpuppy_core.gmcp_register(event.id, "Char")

    # Handle the "Char.Vitals" GMCP message
    @on_gmcp("Char.Vitals")
    async def gmcp_vitals(session_id: mudpuppy_core.SessionId, data: Any):
        hp = data.get("hp", 0)
        printf(f"gmcp_vitals: session {session_id} character has {hp} hp")
    ```
    """
    ...

def on_new_session(
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    Decorator to register an async `mudpuppy_core.EventHandler` function as a handler for the
    `mudpuppy_core.EventType.NewSession` event.

    The decorated function will be called with a `mudpuppy_core.Event.NewSession`
    object when a new session is created.

    The decorated function **must** be async or an error will be produced by
    the decorator.

    If `module` is `None`, then `EventHandler.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    Example:
    ```python
    @on_new_session()
    async def new_session_handler(event: mudpuppy_core.Event):
        assert instance(event, mudpuppy_core.Event.NewSession)
        print(f"new {event.mud} session created: {event.id}")
    ```
    """
    ...

def on_mud_new_session(
    mud_name: Union[str, list[str]],
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    The same as `on_new_session()` but will only be invoked when the event
    occurs for a session with `mudpuppy_core.SessionInfo.mud_name` equal to one
    of the specified `mud_name`'s.

    Example:
    ```python
    @on_mud_new_session("Test (TLS)")
    async def new_session_handler(event: mudpuppy_core.Event):
        assert isinstance(event, mudpuppy_core.Event.NewSession)
        assert event.mud.name == "Test (TLS)"
        print(f"new {event.mud} session created: {event.id}")
    ```
    """
    ...

def on_new_session_or_reload(
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    Decorator to register an async `mudpuppy_core.EventHandler` function as a handler for both
    `mudpuppy_core.EventType.NewSession` and `mudpuppy_core.EventType.ResumeSession`
    events.

    The decorated function will be called with a `mudpuppy_core.Event.NewSession`
    or `mudpuppy_core.Event.ResumeSession` object when a new session is created,
    or when the module is reloaded for existing sessions.

    The decorated function **must** be async or an error will be produced by
    the decorator.

    If `module` is `None`, then `EventHandler.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    Example:
    ```python
    @on_new_session_or_reload()
    async def new_or_resumed_handler(event: mudpuppy_core.Event):
        if isinstance(event, mudpuppy_core.Event.ResumeSession):
            print(f"resuming session: {event.id}")
        elif isinstance(event, mudpuppy_core.Event.NewSession):
            print(f"new {event.mud} session created: {event.id}")
    ```
    """
    ...

def on_mud_new_session_or_reload(
    mud_name: Union[str, list[str]],
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    The same as `on_new_session_or_reload()` but will only be invoked when the event
    occurs for a session with `mudpuppy_core.SessionInfo.mud_name` equal to one
    of the specified `mud_name`'s.
    """
    ...

def on_connected(
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    Decorator to register an async `mudpuppy_core.EventHandler` function as a handler for the
    `mudpuppy_core.EventType.Connection` events that indicate the new connection
     `mudpuppy_core.Status` is `mudpuppy_core.Status.Connected`.

    The decorated function will be called with a `mudpuppy_core.Event.Connection`
    object when a session is connected.

    The decorated function **must** be async or an error will be produced by
    the decorator.

    If `module` is `None`, then `EventHandler.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    Example:
    ```python
    @on_connected()
    async def connected_handler(event: mudpuppy_core.Event):
        assert isinstance(event, mudpuppy_core.Event.Connection)
        assert isinstance(event.status, mudpuppy_core.Status.Connected)
        print(f"session {event.id} connected: {event.status}")
    ```
    """
    ...

def on_mud_connected(
    mud_name: Union[str, list[str]],
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    The same as `on_connected()` but will only be invoked when the event
    occurs for a session with `mudpuppy_core.SessionInfo.mud_name` equal to one
    of the specified `mud_name`'s.
    """
    ...

def on_disconnected(
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    Decorator to register an async `mudpuppy_core.EventHandler` function as a handler for the
    `mudpuppy_core.EventType.Connection` events that indicate the new connection
     `mudpuppy_core.Status` is `mudpuppy_core.Status.Disconnected`.

    The decorated function will be called with a `mudpuppy_core.Event.Connection`
    object when a session is disconnected.

    The decorated function **must** be async or an error will be produced by
    the decorator.

    If `module` is `None`, then `EventHandler.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    Example:
    ```python
    @on_disconnected()
    async def disconnected_handler(event: mudpuppy_core.Event):
        assert isinstance(event, mudpuppy_core.Event.Connection)
        assert isinstance(event.status, mudpuppy_core.Status.Disconnected)
        print(f"session {event.id} disconnected")
    ```
    """
    ...

def on_mud_disconnected(
    mud_name: Union[str, list[str]],
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.EventHandler], mudpuppy_core.EventHandler]:
    """
    The same as `on_disconnected()` but will only be invoked when the event
    occurs for a session with `mudpuppy_core.SessionInfo.mud_name` equal to one
    of the specified `mud_name`'s.
    """
    ...

def alias(
    *,
    pattern: str,
    name: Optional[str] = None,
    expansion: Optional[str] = None,
    mud_name: Optional[Union[str, list[str]]] = None,
    module: Optional[str] = None,
    max_hits: Optional[int] = None,
) -> Callable[[mudpuppy_core.AliasCallable], mudpuppy_core.AliasCallable]:
    """
    Decorator to register an async `mudpuppy_core.AliasCallable` function as an alias handler for
    a specific pattern. A `mudpuppy_core.Alias` will be automatically created for the decorated
    function when `mudpuppy_core.EventType.NewSession` and
    `mudpuppy_core.EventType.ResumeSession` events occur. If no `name` is provided, the name of
    the decorated function is used as the alias name.

    When input is provided matching the compiled regexp `pattern` the decorated function
    will be invoked with the `mudpuppy_core.SessionId` of the session that received the
    input, the `mudpuppy_core.AliasId` of the `mudpuppy_core.Alias` that matched, the
    input line that matched, and a list of captured groups from the pattern (if any).

    The `pattern` must be a valid regular expression. You can read more about the allowed
    syntax [in the `regexp` crate docs](https://docs.rs/regex/latest/regex/#syntax). Note
    that for performance reasons the Python `regex` module is not used - the pattern
    is used to create a Rust regular expression object. By using group syntax you will
    receive the matched groups as a separate list argument to the decorated function.

    An optional `expansion` string may be provided. When the alias matches the
    `expansion` will be sent to the MUD as input. This is a convenient shorthand
    for writing `await mudpuppy_core.MudpuppyCore.send_line(session_id, expansion)`
    in the body of your `mudpuppy_core.AliasCallable`.

    An optional `max_hits` integer may be provided. If set, the alias will only be
    invoked `max_hits` times before being automatically disabled with
    `mudpuppy_core.MudpuppyCore.disable_alias()`.

    If a `mud_name`, or list of `mud_name`'s are provided then the alias will only be
    registered for sessions with the specified `mud_name`'s.

    If `module` is `None`, then `AliasCallable.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    If you want more control over the alias creation process (for example, because
    you want to add an alias sometime after session creation or wish to store the
    `mudpuppy_core.AliasId` of the created alias) you should prefer to instantiate
    your own `mudpuppy_core.AliasConfig` to use with
    `mudpuppy_core.MudpuppyCore.new_alias()`.

    Example:
    ```python
    import asyncio
    from mudpuppy_core import mudpuppy_core, SessionId, AliasId

    @alias(mud_name="Test (TLS)", pattern="^kill (.*)$")
    async def start_combat(session_id: SessionId, _alias_id: AliasId, _line: str, groups: list[str]):
        assert len(groups) == 1
        target = groups[0]
        await mudpuppy_core.send(session_id, f"backstab {target}")
        await asyncio.sleep(10)
        await mudpuppy_core.send(session_id, f"steal from {target}")
    ```
    """

def trigger(
    *,
    pattern: str,
    name: Optional[str] = None,
    gag: bool = False,
    strip_ansi: bool = True,
    prompt: bool = False,
    expansion: Optional[str] = None,
    mud_name: Optional[Union[str, list[str]]] = None,
    module: Optional[str] = None,
    max_hits: Optional[int] = None,
) -> Callable[[mudpuppy_core.TriggerCallable], mudpuppy_core.TriggerCallable]:
    """
    Decorator to register an async `mudpuppy_core.TriggerCallable` function as a trigger handler for
    a specific pattern. A `mudpuppy_core.Trigger` will be automatically created for the decorated
    function when `mudpuppy_core.EventType.NewSession` and
    `mudpuppy_core.EventType.ResumeSession` events occur. If no `name` is provided, the name of
    the decorated function is used as the trigger name.

    When output is received matching the compiled regexp `pattern` the decorated function
    will be invoked with the `mudpuppy_core.SessionId` of the session that received the
    output, the `mudpuppy_core.TriggerId` of the `mudpuppy_core.Trigger` that matched, the
    output line that matched, and a list of captured groups from the pattern (if any).

    The `pattern` must be a valid regular expression. You can read more about the allowed
    syntax [in the `regexp` crate docs](https://docs.rs/regex/latest/regex/#syntax). Note
    that for performance reasons the Python `regex` module is not used - the pattern
    is used to create a Rust regular expression object. By using group syntax you will
    receive the matched groups as a separate list argument to the decorated function.

    If `gag` is `True`, then the line that matched the trigger's `pattern` will be
    gagged and **not displayed** in the MUD output area. This is useful for suppressing
    text from the MUD server you don't want to see (e.g. because it's spammy, or because
    you're post-processing it into something easier to understand that you'll output
    manually).

    If `strip_ansi` is `True` (default), then the line that matched the trigger's `pattern`
    will have ANSI escape codes stripped before being passed to the decorated function.
    You typically want `strip_ansi` enabled so that your `pattern` regexp doesn't have to
    match the ANSI colour codes that may be decorating output. If you **do** want to write
    a pattern that only matches text in a specific colour you can set `strip_ansi=False`.

    If `prompt` is `True`, then the trigger will only match if the line was detected as
    a prompt. The `pattern` must also match. Writing triggers with `prompt=True` is an
    alternative to registering a `mudpuppy_core.EventType.Prompt` event handlers and can
    be useful if you want to gag a prompt by setting both `prompt=True` and `gag=True`
    and writing a matching `pattern`.

    An optional `expansion` string may be provided. When the trigger matches the
    `expansion` will be sent to the MUD as input.  This is a convenient shorthand
    for writing `await mudpuppy_core.MudpuppyCore.send_line(session_id, expansion)`
    in the body of your `mudpuppy_core.TriggerCallable`.

    An optional `max_hits` integer may be provided. If set, the trigger will only be
    invoked `max_hits` times before being automatically disabled with
    `mudpuppy_core.MudpuppyCore.disable_trigger()`.

    If a `mud_name`, or list of `mud_name`'s are provided then the trigger will only be
    registered for sessions with the specified `mud_name`'s.

    If `module` is `None`, then `TriggerCallable.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    If you want more control over the trigger creation process (for example, because
    you want to add a trigger sometime after session creation or wish to store the
    `mudpuppy_core.TriggerId` of the created trigger) you should prefer to instantiate
    your own `mudpuppy_core.TriggerConfig` to use with
    `mudpuppy_core.MudpuppyCore.new_trigger()`.

    Example:
    ```python
    from mudpuppy_core import mudpuppy_core, SessionId, TriggerId
    from mudpuppy import trigger

    @trigger(pattern="^You wear (.*)$")
    async def auto_keep(
        session_id: SessionId, _trigger_id: TriggerId, _line: str, groups: list[str]
    ):
        await mudpuppy_core.send_line(session_id, f"keep {groups[0]}")
    ```
    """
    ...

def highlight(
    *,
    pattern: str,
    name: Optional[str] = None,
    strip_ansi: bool = True,
    mud_name: Optional[Union[str, list[str]]] = None,
    module: Optional[str] = None,
) -> Callable[[mudpuppy_core.HighlightCallable], mudpuppy_core.HighlightCallable]:
    """
    Decorator to register a **non-async** `mudpuppy_core.HighlightCallable` function as a highlight
    handler for a specific pattern. A `mudpuppy_core.Trigger` for the highlight
    will be automatically created for the decorated function when
    `mudpuppy_core.EventType.NewSession` and `mudpuppy_core.EventType.ResumeSession`
    events occur. If no `name` is provided, the name of the decorated function is used as
    the highlight trigger name.

    When output is received matching the compiled regexp `pattern` the decorated function
    will be invoked with the `mudpuppy_core.MudLine` that matched the `pattern`,
    and a list of captured groups from the pattern (if any).

    The `pattern` must be a valid regular expression. You can read more about the allowed
    syntax [in the `regexp` crate docs](https://docs.rs/regex/latest/regex/#syntax). Note
    that for performance reasons the Python `regex` module is not used - the pattern
    is used to create a Rust regular expression object. By using group syntax you will
    receive the matched groups as a separate list argument to the decorated function.

    If `strip_ansi` is `True` (default), then the line that matched the highlight
    trigger's `pattern` will have ANSI escape codes stripped before being passed to the
    decorated function. You typically want `strip_ansi` enabled so that your `pattern`
    regexp doesn't have to match the ANSI colour codes that may be decorating output.
    If you **do** want to write a pattern that only matches text in a specific colour
    you can set `strip_ansi=False`.

    If a `mud_name`, or list of `mud_name`'s are provided then the trigger will only be
    registered for sessions with the specified `mud_name`'s.

    If `module` is `None`, then `HighlightCallable.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    If you want more control over the trigger creation process (for example, because
    you want to add a trigger sometime after session creation or wish to store the
    `mudpuppy_core.TriggerId` of the created trigger) you should prefer to instantiate
    your own `mudpuppy_core.TriggerConfig` to use with
    `mudpuppy_core.MudpuppyCore.add_trigger()`.

    Example:
    ```python
    from mudpuppy import highlight

    @highlight(pattern=r"^(\\d+) solaris$")
    def solaris_highlight(line: mudpuppy_core.MudLine, groups: list[str]):
        assert len(groups) == 1
        # Highlight the total in bold
        hilight = line.__str__().replace(
            groups[0], cformat(f"<bold><yellow>{groups[0]}<reset><yellow>")
        )
        # And then overall line in yellow
        hilight = cformat(f"<yellow>{hilight}<reset>")
        line.set(hilight)
        return line
    ```
    """
    ...

type TimerCallable = Callable[
    [mudpuppy_core.TimerId, Optional[mudpuppy_core.SessionId]], Awaitable[None]
]
"""
An async function that is called when a timer expires. See `mudpuppy.timer()` for the
associated `@timer()` decorator.

The handler is called with:

* the `mudpuppy_core.TimerId` of the timer that expired.
* either `None`, or `mudpuppy_core.SessionId` if the `mudpuppy_core.Timer` was created
  for a specific `mudpuppy_core.SessionId`.

Example:
```python
from mudpuppy_core import mudpuppy_core

async def my_timer_handler(timer_id: mudpuppy_core.TimerId, sesh: Optional[mudpuppy_core.SessionId]):
    timer: mudpuppy_core.Timer = await mudpuppy_core.get_timer(sesh, timer_id)
    print(f"trigger {timer.config.name} has fired")
    if randint(1, 3) == 1:
        print(f"stopping timer {timer_id}")
        await mudpuppy_core.stop_timer(sesh, timer_id)
```
"""

def timer(
    *,
    name: Optional[str] = None,
    milliseconds: int = 0,
    seconds: int = 0,
    minutes: int = 0,
    hours: int = 0,
    max_ticks: Optional[int] = None,
    mud_name: Optional[Union[str, list[str]]] = None,
    module: Optional[str] = None,
) -> Callable[[TimerCallable], TimerCallable]:
    """
    Decorator to register an async `TimerCallable` function as a timer handler run every
    specified duration. A `mudpuppy_core.Timer` will be automatically created for the decorated
    function when `mudpuppy_core.EventType.NewSession` and
    `mudpuppy_core.EventType.ResumeSession` events occur. If no `name` is provided, the name of
    the decorated function is used as the timer name.

    A duration is calculated based on the `milliseconds`, `seconds`, `minutes`, and `hours`
    arguments. After the duration has expired the `TimerCallable` is invoked. This will
    happen over and over until the timer is stopped with `mudpuppy_core.MudpuppyCore.stop_timer()`
    or until the optional `max_ticks` value is reached.

    If a `mud_name`, or list of `mud_name`'s are provided then the timer will only be
    registered for sessions with the specified `mud_name`'s.

    If `module` is `None`, then `TimerCallable.__module__` will be used. The module
    name is used only for unregistering handlers when the module is to be reloaded.
    See `unload_handlers()` for more information.

    If you want more control over the timer creation process (for example, because
    you want to add a timer sometime after session creation or wish to store the
    `mudpuppy_core.TimerId` of the created timer) you should prefer to instantiate
    your own `mudpuppy_core.TimerConfig` to use with
    `mudpuppy_core.MudpuppyCore.new_timer()`.

    Example:
    ```python
    from mudpuppy_core import mudpuppy_core, SessionId, TimerId
    from mudpuppy import timer

    @timer(minutes=10, seconds=30)
    async def auto_save(
        _timer_id: TimerId, session_id: Optional[SessionId]
    ):
        assert session_id is not None
        await mudpuppy_core.send_line(session_id, "save")
    ```
    """
    ...

def trigger_max_hits(
    *, handler: mudpuppy_core.TriggerCallable, max_hits: int = 1
) -> mudpuppy_core.TriggerCallable:
    """
    Wrap a `mudpuppy_core.TriggerCallable` to limit the number of times it can be invoked.

    Example:
    ```python
    from mudpuppy_core import mudpuppy_core, TriggerConfig
    from mudpuppy import trigger_max_hits

    async def my_trigger_handler(_session_id: mudpuppy_core.SessionId, trigger_id: mudpuppy_core.TriggerId, line: str, _groups: list[str]):
        print(f"trigger {trigger_id} matched line: {line}")

    # Wrap the handler w/ mudpuppy.trigger_max_hits to limit to 3 hits
    trigger_config = TriggerConfig("^test$", "test trigger", handler=trigger_max_hits(handler=my_trigger_handler, max_hits=3))
    trigger_id = await mudpuppy_core.new_trigger(trigger_config)
    ```
    """
    ...

def alias_max_hits(
    *, handler: mudpuppy_core.AliasCallable, max_hits: int = 1
) -> mudpuppy_core.AliasCallable:
    """
    Wrap a `mudpuppy_core.AliasCallable` to limit the number of times it can be invoked.

    Example:
    ```python
    from mudpuppy_core import mudpuppy_core, AliasConfig
    from mudpuppy import alias_max_hits

    async def my_alias_handler(_session_id: mudpuppy_core.SessionId, alias_id: mudpuppy_core.AliasId, line: str, _groups: list[str]):
        print(f"alias {alias_id} matched line: {line}")

    # Wrap the handler w/ mudpuppy.alias_max_hits to limit to 3 hits
    alias_config = AliasConfig("^test$", "test alias", handler=alias_max_hits(handler=my_alias_handler, max_hits=3))
    alias_id = await mudpuppy_core.new_alias(alias_config)
    ```
    """
    ...

def unload_handlers(module: str):
    """
    Unregister all event handlers, GMCP handlers, triggers, aliases, and timers
    registered by the specified module.

    This is useful to call ahead of a module reload to ensure that no handlers
    are left registered for the old module.

    The `module` name should be the same as the `module` argument passed to the
    various handler decorators at the time of registration.
    """
    ...
