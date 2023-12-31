import inspect
import json
import logging
from typing import Any, Awaitable, Callable, List, Optional, Union

from mudpuppy_core import (
    AliasConfig,
    AliasId,
    Event,
    EventType,
    MudLine,
    SessionId,
    Status,
    TimerConfig,
    TriggerConfig,
    TriggerId,
    event_handlers,
    mudpuppy_core,
)

EventHandler = Callable[[Event], Awaitable[None]]
GmcpHandler = Callable[[SessionId, Any], Awaitable[None]]

logging.debug(f"Mudpuppy Python module loaded: {mudpuppy_core}")


# It's easy to forget to add the async keyword to a function that should be async.
def ensure_async(handler: Callable) -> None:
    if not inspect.iscoroutinefunction(handler):
        raise TypeError(
            f"The handler function '{handler.__name__}' must be an async function"
        )


def on_event(event_type: Union[EventType, List[EventType]], module=None):
    def on_event_decorator(handler: EventHandler):
        ensure_async(handler)
        handler_module = module or handler.__module__
        if isinstance(event_type, list):
            for et in event_type:
                event_handlers.add_handler(et, handler, handler_module)
        else:
            event_handlers.add_handler(event_type, handler, handler_module)
        return handler

    return on_event_decorator


def on_gmcp(package: str, module=None):
    def on_gmcp_decorator(handler: GmcpHandler):
        ensure_async(handler)

        @on_event(EventType.GmcpMessage, module=module or handler.__module__)
        async def gmcp_message_wrapper(event: Event):
            if event.package == package:
                await handler(event.id, json.loads(event.json))

        return gmcp_message_wrapper

    return on_gmcp_decorator


def on_new_session(module=None):
    def decorator(handler: EventHandler):
        logging.debug(
            f"setting up on_new_session handler for {module} ({handler.__module__}) -> {handler.__name__}"
        )
        ensure_async(handler)

        @on_event(EventType.NewSession, module=module or handler.__module__)
        async def on_new_session_decorator(event: Event):
            await handler(event)

        return on_new_session_decorator

    return decorator


def on_new_session_or_reload(module=None):
    def decorator(handler: EventHandler):
        logging.debug(
            f"setting up on_new_session_or_reload handler for {module} ({handler.__module__}) -> {handler.__name__}"
        )
        ensure_async(handler)

        @on_event(
            [EventType.NewSession, EventType.ResumeSession],
            module=module or handler.__module__,
        )
        async def on_new_session_or_reload_decorator(event: Event):
            await handler(event)

        return on_new_session_or_reload_decorator

    return decorator


def on_connected(module=None):
    def decorator(handler: EventHandler):
        ensure_async(handler)

        @on_event(EventType.Connection, module=module or handler.__module__)
        async def on_connected_decorator(event: Event):
            if isinstance(event.status, Status.Connected):
                await handler(event)

        return on_connected_decorator

    return decorator


def on_disconnected(module=None):
    def decorator(handler: EventHandler):
        ensure_async(handler)

        @on_event(EventType.Connection, module=module or handler.__module__)
        async def on_disconnected_decorator(event: Event):
            if isinstance(event.status, Status.Disconnected):
                await handler(event)

        return on_disconnected_decorator

    return decorator


def on_mud_event(
    mud_name: str, event_type: Union[EventType, List[EventType]], module=None
):
    def on_mud_event_decorator(handler: EventHandler):
        ensure_async(handler)

        @on_event(event_type, module=module or handler.__module__)
        async def on_mud_event_wrapper(event: Event):
            sesh_id = event.session_id()
            if sesh_id is None:
                return  # Global event - skip
            session_info_data = await mudpuppy_core.session_info(sesh_id)
            if session_info_data.mud_name == mud_name:
                await handler(event)

        return on_mud_event_wrapper

    return on_mud_event_decorator


def on_mud_new_session(mud_name: str, module=None):
    def on_mud_new_session_decorator(handler: EventHandler):
        ensure_async(handler)

        @on_mud_event(
            mud_name, EventType.NewSession, module=module or handler.__module__
        )
        async def new_session_event_wrapper(event: Event):
            await handler(event)

        return new_session_event_wrapper

    return on_mud_new_session_decorator


def on_mud_new_session_or_reload(mud_name: str, module=None):
    def on_mud_new_session_or_reload_decorator(handler: EventHandler):
        ensure_async(handler)

        @on_mud_event(
            mud_name,
            [EventType.NewSession, EventType.ResumeSession],
            module=module or handler.__module__,
        )
        async def new_session_or_reload_event_wrapper(event: Event):
            await handler(event)

        return new_session_or_reload_event_wrapper

    return on_mud_new_session_or_reload_decorator


def on_mud_connected(mud_name: str, module=None):
    def on_mud_connected_decorator(handler: EventHandler):
        ensure_async(handler)

        @on_mud_event(
            mud_name, EventType.Connection, module=module or handler.__module__
        )
        async def connected_event_wrapper(event: Event):
            if isinstance(event.status, Status.Connected):
                await handler(event)

        return connected_event_wrapper

    return on_mud_connected_decorator


def on_mud_disconnected(mud_name: str, module=None):
    def on_mud_disconnected_decorator(handler: EventHandler):
        ensure_async(handler)

        @on_mud_event(
            mud_name, EventType.Connection, module=module or handler.__module__
        )
        async def disconnected_event_wrapped(event: Event):
            if isinstance(event.status, Status.Disconnected):
                await handler(event)

        return disconnected_event_wrapped

    return on_mud_disconnected_decorator


AliasCallable = Callable[[SessionId, AliasId, str, Any], Awaitable[None]]


def alias(
    *,
    pattern: str,
    name: str,
    expansion: Optional[str] = None,
    mud_name: Optional[str] = None,
    module: Optional[str] = None,
):
    def alias_decorator(handler: Callable):
        if pattern.strip() == "" or name.strip() == "":
            raise ValueError("Pattern and name must be non-empty")

        ensure_async(handler)

        alias_config = AliasConfig(pattern, name)
        alias_config.expansion = expansion
        alias_config.callback = handler

        if mud_name:

            @on_mud_new_session_or_reload(mud_name, module=module or handler.__module__)
            async def alias_event_wrapper(event: Event):
                alias_id = await mudpuppy_core.new_alias(
                    event.id, alias_config, module=module or handler.__module__
                )
                if alias_id:
                    logging.debug(
                        f"{mud_name} alias with name {name} created with ID: {alias_id}"
                    )

            return alias_event_wrapper
        else:

            @on_new_session_or_reload(module=module or handler.__module__)
            async def global_alias_event_wrapper(event: Event):
                alias_id = await mudpuppy_core.new_alias(
                    event.id, alias_config, module=module or handler.__module__
                )
                if alias_id:
                    logging.debug(
                        f"Global alias with name {name} created with ID: {alias_id}"
                    )

            return global_alias_event_wrapper

    return alias_decorator


TriggerCallable = Callable[[SessionId, TriggerId, str, Any], Awaitable[None]]


def trigger(
    *,
    pattern: str,
    name: str,
    gag: bool = False,
    strip_ansi: bool = True,
    expansion: Optional[str] = None,
    mud_name: Optional[str] = None,
    module: Optional[str] = None,
):
    def trigger_decorator(handler: TriggerCallable):
        if pattern.strip() == "" or name.strip() == "":
            raise ValueError("pattern and name must be non-empty")

        ensure_async(handler)

        trigger_config = TriggerConfig(pattern, name)
        trigger_config.gag = gag
        trigger_config.strip_ansi = strip_ansi
        trigger_config.expansion = expansion
        trigger_config.callback = handler

        if mud_name:

            @on_mud_new_session_or_reload(mud_name, module=module or handler.__module__)
            async def trigger_event_wrapper(event: Event):
                trigger_id = await mudpuppy_core.new_trigger(
                    event.id, trigger_config, module=module or handler.__module__
                )
                if trigger_id:
                    logging.debug(
                        f"{mud_name} trigger with name {name} created with ID: {trigger_id}"
                    )

            return trigger_event_wrapper
        else:

            @on_new_session_or_reload(module=module or handler.__module__)
            async def global_trigger_event_wrapper(event: Event):
                trigger_id = await mudpuppy_core.new_trigger(
                    event.id, trigger_config, module=module or handler.__module__
                )
                if trigger_id:
                    logging.debug(
                        f"Global trigger with name {name} created with ID: {trigger_id}"
                    )

            return global_trigger_event_wrapper

    return trigger_decorator


# Note: Not async!
HighlightCallable = Callable[[SessionId, TriggerId, str, Any], MudLine]


def highlight(
    *,
    pattern: str,
    name: str,
    strip_ansi: bool = True,
    mud_name: Optional[str] = None,
    module: Optional[str] = None,
):
    def highlight_decorator(handler: TriggerCallable):
        if pattern.strip() == "" or name.strip() == "":
            raise ValueError("pattern and name must be non-empty")

        trigger_config = TriggerConfig(pattern, name)
        trigger_config.strip_ansi = strip_ansi
        trigger_config.highlight = handler

        if mud_name:

            @on_mud_new_session_or_reload(mud_name, module=module or handler.__module__)
            async def highlight_event_wrapper(event: Event):
                trigger_id = await mudpuppy_core.new_trigger(
                    event.id, trigger_config, module=module or handler.__module__
                )
                if trigger_id:
                    logging.debug(
                        f"{mud_name} highlight trigger with name {name} created with ID: {trigger_id}"
                    )

            return highlight_event_wrapper
        else:

            @on_new_session_or_reload(module=module or handler.__module__)
            async def global_highlight_event_wrapper(event: Event):
                trigger_id = await mudpuppy_core.new_trigger(
                    event.id, trigger_config, module=module or handler.__module__
                )
                if trigger_id:
                    logging.debug(
                        f"Global highlight trigger with name {name} created with ID: {trigger_id}"
                    )

            return global_highlight_event_wrapper

    return highlight_decorator


def timer(
    *,
    name: str,
    milliseconds: int = 0,
    seconds: int = 0,
    minutes: int = 0,
    hours: int = 0,
    max_ticks: Optional[int] = None,
    mud_name: Optional[str] = None,
    module: Optional[str] = None,
):
    if not name.strip():
        raise ValueError("The 'name' argument must be a non-empty string.")

    total_delay_ms = (
        milliseconds + (seconds * 1000) + (minutes * 60 * 1000) + (hours * 3600 * 1000)
    )

    logging.debug(f"timer {name} will run every {total_delay_ms} ms")
    if total_delay_ms <= 0:
        raise ValueError("The total duration must be greater than zero.")

    def timer_decorator(handler):
        ensure_async(handler)

        timer_config = TimerConfig(name, total_delay_ms, handler)
        if max_ticks:
            timer_config.max_ticks = max_ticks
        logging.debug(f"config: {timer_config}")

        if mud_name:

            @on_mud_new_session_or_reload(mud_name, module=module or handler.__module__)
            async def mud_timer_event_wrapper(event: Event):
                timer_config.session_id = event.id
                timer_id = await mudpuppy_core.new_timer(
                    timer_config, module=module or handler.__module__
                )
                if timer_id:
                    logging.debug(
                        f"{mud_name} timer with name '{name}' created with ID: {timer_id}"
                    )

            return mud_timer_event_wrapper

        else:

            @on_new_session_or_reload(module=module or handler.__module__)
            async def global_timer_event_wrapper(_event: Event):
                timer_id = await mudpuppy_core.new_timer(
                    timer_config, module=module or handler.__module__
                )
                if timer_id:
                    logging.debug(
                        f"Global timer with name '{name}' created with ID: {timer_id}"
                    )

            return global_timer_event_wrapper

    return timer_decorator


def unload_handlers(module: str):
    for event_type in event_handlers.get_handler_events():
        handlers_list = event_handlers.get_handlers(event_type)
        logging.debug(
            f"event type {event_type} had {len(handlers_list)} handlers before unloading {module} handlers"
        )
        handlers_list[:] = [h for h in handlers_list if h[1] != module]
        logging.debug(f"event type {event_type} now has {len(handlers_list)} handlers")
