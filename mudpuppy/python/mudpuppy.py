import inspect
import json
import logging
import warnings
from typing import Any, Awaitable, Callable, List, Optional, Union

from mudpuppy_core import (
    AliasConfig,
    OutputItem,
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
def __ensure_async(handler: Callable) -> None:
    if not inspect.iscoroutinefunction(handler):
        raise TypeError(
            f"The handler function '{handler.__name__}' must be an async function"
        )


def on_event(event_type: Union[EventType, List[EventType]], module=None):
    def on_event_decorator(handler: EventHandler):
        __ensure_async(handler)
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
        __ensure_async(handler)

        @on_event(EventType.GmcpMessage, module=module or handler.__module__)
        async def gmcp_message_wrapper(event: Event):
            assert isinstance(event, Event.GmcpMessage)
            if event.package == package:
                await handler(event.id, json.loads(event.json))

        return gmcp_message_wrapper

    return on_gmcp_decorator


def on_new_session(module=None):
    def decorator(handler: EventHandler):
        logging.debug(
            f"setting up on_new_session handler for {module} ({handler.__module__}) -> {handler.__name__}"
        )
        __ensure_async(handler)

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
        __ensure_async(handler)

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
        __ensure_async(handler)

        @on_event(EventType.Connection, module=module or handler.__module__)
        async def on_connected_decorator(event: Event):
            assert isinstance(event, Event.Connection)
            if isinstance(event.status, Status.Connected):
                await handler(event)

        return on_connected_decorator

    return decorator


def on_disconnected(module=None):
    def decorator(handler: EventHandler):
        __ensure_async(handler)

        @on_event(EventType.Connection, module=module or handler.__module__)
        async def on_disconnected_decorator(event: Event):
            assert isinstance(event, Event.Connection)
            if isinstance(event.status, Status.Disconnected):
                await handler(event)

        return on_disconnected_decorator

    return decorator


def on_mud_event(
    mud_name: Union[str, List[str]],
    event_type: Union[EventType, List[EventType]],
    module=None,
):
    def on_mud_event_decorator(handler: EventHandler):
        __ensure_async(handler)

        @on_event(event_type, module=module or handler.__module__)
        async def on_mud_event_wrapper(event: Event):
            sesh_id = event.session_id()
            if sesh_id is None:
                return  # Global event - skip
            session_info_data = await mudpuppy_core.session_info(sesh_id)
            if isinstance(mud_name, str):
                if session_info_data.mud_name == mud_name:
                    await handler(event)
            elif isinstance(mud_name, list):
                if session_info_data.mud_name in mud_name:
                    await handler(event)

        return on_mud_event_wrapper

    return on_mud_event_decorator


def on_mud_new_session(mud_name: Union[str, List[str]], module=None):
    def on_mud_new_session_decorator(handler: EventHandler):
        __ensure_async(handler)

        @on_mud_event(
            mud_name, EventType.NewSession, module=module or handler.__module__
        )
        async def new_session_event_wrapper(event: Event):
            await handler(event)

        return new_session_event_wrapper

    return on_mud_new_session_decorator


def on_mud_new_session_or_reload(mud_name: Union[str, List[str]], module=None):
    def on_mud_new_session_or_reload_decorator(handler: EventHandler):
        __ensure_async(handler)

        @on_mud_event(
            mud_name,
            [EventType.NewSession, EventType.ResumeSession],
            module=module or handler.__module__,
        )
        async def new_session_or_reload_event_wrapper(event: Event):
            await handler(event)

        return new_session_or_reload_event_wrapper

    return on_mud_new_session_or_reload_decorator


def on_mud_connected(mud_name: Union[str, List[str]], module=None):
    def on_mud_connected_decorator(handler: EventHandler):
        __ensure_async(handler)

        @on_mud_event(
            mud_name, EventType.Connection, module=module or handler.__module__
        )
        async def connected_event_wrapper(event: Event):
            assert isinstance(event, Event.Connection)
            if isinstance(event.status, Status.Connected):
                await handler(event)

        return connected_event_wrapper

    return on_mud_connected_decorator


def on_mud_disconnected(mud_name: Union[str, List[str]], module=None):
    def on_mud_disconnected_decorator(handler: EventHandler):
        __ensure_async(handler)

        @on_mud_event(
            mud_name, EventType.Connection, module=module or handler.__module__
        )
        async def disconnected_event_wrapped(event: Event):
            assert isinstance(event, Event.Connection)
            if isinstance(event.status, Status.Disconnected):
                await handler(event)

        return disconnected_event_wrapped

    return on_mud_disconnected_decorator


AliasCallable = Callable[[SessionId, AliasId, str, Any], Awaitable[None]]


def alias(
    *,
    pattern: str,
    name: Optional[str] = None,
    expansion: Optional[str] = None,
    mud_name: Optional[Union[str, List[str]]] = None,
    module: Optional[str] = None,
    max_hits: Optional[int] = None,
):
    def alias_decorator(handler: Callable):
        alias_name = name or handler.__name__
        if pattern.strip() == "" or alias_name.strip() == "":
            raise ValueError("Pattern and name must be non-empty")

        __ensure_async(handler)

        if max_hits is not None:
            handler = alias_max_hits(handler=handler, max_hits=max_hits)

        alias_config = AliasConfig(
            pattern, alias_name, expansion=expansion, callback=handler
        )

        if mud_name:

            @on_mud_new_session_or_reload(mud_name, module=module or handler.__module__)
            async def alias_event_wrapper(event: Event):
                assert isinstance(event, Event.NewSession) or isinstance(
                    event, Event.ResumeSession
                )
                alias_id = await mudpuppy_core.new_alias(
                    event.id, alias_config, module=module or handler.__module__
                )
                if alias_id:
                    logging.debug(
                        f"{mud_name} alias with name {alias_name} created with ID: {alias_id}"
                    )

            return alias_event_wrapper
        else:

            @on_new_session_or_reload(module=module or handler.__module__)
            async def global_alias_event_wrapper(event: Event):
                assert isinstance(event, Event.NewSession) or isinstance(
                    event, Event.ResumeSession
                )
                alias_id = await mudpuppy_core.new_alias(
                    event.id, alias_config, module=module or handler.__module__
                )
                if alias_id:
                    logging.debug(
                        f"Global alias with name {alias_name} created with ID: {alias_id}"
                    )

            return global_alias_event_wrapper

    return alias_decorator


TriggerCallable = Callable[[SessionId, TriggerId, str, Any], Awaitable[None]]


def trigger(
    *,
    pattern: str,
    name: Optional[str] = None,
    gag: bool = False,
    strip_ansi: bool = True,
    prompt: bool = False,
    expansion: Optional[str] = None,
    mud_name: Optional[Union[str, List[str]]] = None,
    module: Optional[str] = None,
    max_hits: Optional[int] = None,
):
    def trigger_decorator(handler: TriggerCallable):
        trigger_name = name or handler.__name__
        if pattern.strip() == "" or trigger_name.strip() == "":
            raise ValueError("pattern and name must be non-empty")

        __ensure_async(handler)

        if max_hits is not None:
            handler = trigger_max_hits(handler=handler, max_hits=max_hits)

        trigger_config = TriggerConfig(
            pattern,
            trigger_name,
            gag=gag,
            prompt=prompt,
            strip_ansi=strip_ansi,
            expansion=expansion,
            callback=handler,
        )

        if mud_name:

            @on_mud_new_session_or_reload(mud_name, module=module or handler.__module__)
            async def trigger_event_wrapper(event: Event):
                assert isinstance(event, Event.NewSession) or isinstance(
                    event, Event.ResumeSession
                )
                trigger_id = await mudpuppy_core.new_trigger(
                    event.id, trigger_config, module=module or handler.__module__
                )
                if trigger_id:
                    logging.debug(
                        f"{mud_name} trigger with name {trigger_name} created with ID: {trigger_id}"
                    )

            return trigger_event_wrapper
        else:

            @on_new_session_or_reload(module=module or handler.__module__)
            async def global_trigger_event_wrapper(event: Event):
                assert isinstance(event, Event.NewSession) or isinstance(
                    event, Event.ResumeSession
                )
                trigger_id = await mudpuppy_core.new_trigger(
                    event.id, trigger_config, module=module or handler.__module__
                )
                if trigger_id:
                    logging.debug(
                        f"Global trigger with name {trigger_name} created with ID: {trigger_id}"
                    )

            return global_trigger_event_wrapper

    return trigger_decorator


# Note: Not async!
HighlightCallable = Callable[[MudLine, list[str]], MudLine]


def highlight(
    *,
    pattern: str,
    name: Optional[str] = None,
    strip_ansi: bool = True,
    mud_name: Optional[Union[str, list[str]]] = None,
    module: Optional[str] = None,
):
    def highlight_decorator(handler: HighlightCallable):
        highlight_name = name or handler.__name__
        if pattern.strip() == "" or highlight_name.strip() == "":
            raise ValueError("pattern and name must be non-empty")

        trigger_config = TriggerConfig(
            pattern, highlight_name, strip_ansi=strip_ansi, highlight=handler
        )

        if mud_name:

            @on_mud_new_session_or_reload(mud_name, module=module or handler.__module__)
            async def highlight_event_wrapper(event: Event):
                assert isinstance(event, Event.NewSession) or isinstance(
                    event, Event.ResumeSession
                )
                trigger_id = await mudpuppy_core.new_trigger(
                    event.id, trigger_config, module=module or handler.__module__
                )
                if trigger_id:
                    logging.debug(
                        f"{mud_name} highlight trigger with name {highlight_name} created with ID: {trigger_id}"
                    )

            return highlight_event_wrapper
        else:

            @on_new_session_or_reload(module=module or handler.__module__)
            async def global_highlight_event_wrapper(event: Event):
                assert isinstance(event, Event.NewSession) or isinstance(
                    event, Event.ResumeSession
                )
                trigger_id = await mudpuppy_core.new_trigger(
                    event.id, trigger_config, module=module or handler.__module__
                )
                if trigger_id:
                    logging.debug(
                        f"Global highlight trigger with name {highlight_name} created with ID: {trigger_id}"
                    )

            return global_highlight_event_wrapper

    return highlight_decorator


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
):
    total_delay_ms = (
        milliseconds + (seconds * 1000) + (minutes * 60 * 1000) + (hours * 3600 * 1000)
    )

    if total_delay_ms <= 0:
        raise ValueError("The total duration must be greater than zero.")

    def timer_decorator(handler):
        timer_name = name or handler.__name__
        if not timer_name.strip():
            raise ValueError("The 'name' argument must be a non-empty string.")

        __ensure_async(handler)

        logging.debug(f"timer {timer_name} will run every {total_delay_ms} ms")

        timer_config = TimerConfig(timer_name, total_delay_ms, handler)
        if max_ticks:
            timer_config.max_ticks = max_ticks
        logging.debug(f"config: {timer_config}")

        if mud_name:

            @on_mud_new_session_or_reload(mud_name, module=module or handler.__module__)
            async def mud_timer_event_wrapper(event: Event):
                assert isinstance(event, Event.NewSession) or isinstance(
                    event, Event.ResumeSession
                )
                timer_config.session_id = event.id
                timer_id = await mudpuppy_core.new_timer(
                    timer_config, module=module or handler.__module__
                )
                if timer_id:
                    logging.debug(
                        f"{mud_name} timer with name '{timer_name}' created with ID: {timer_id}"
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
                        f"Global timer with name '{timer_name}' created with ID: {timer_id}"
                    )

            return global_timer_event_wrapper

    return timer_decorator


def unload_handlers(module: str):
    for event_type in event_handlers.get_handler_events():
        handlers_list = event_handlers.get_handlers(event_type)
        if handlers_list is None:
            continue
        logging.debug(
            f"event type {event_type} had {len(handlers_list)} handlers before unloading {module} handlers"
        )
        handlers_list[:] = [h for h in handlers_list if h[1] != module]  # type: ignore
        logging.debug(f"event type {event_type} now has {len(handlers_list)} handlers")


def custom_showwarning(message, category, filename, lineno, _file=None, _line=None):
    # Log the warning to the mudpuppy log file.
    full_msg = f"{filename}:{lineno}: {category.__name__}: {message}"
    logging.warning(full_msg)

    async def handle_runtime_warning(full_msg: str):
        sesh_id = await mudpuppy_core.active_session_id()
        if sesh_id is None:
            return

        await mudpuppy_core.add_outputs(
            sesh_id,
            [OutputItem.failed_command_result(line) for line in full_msg.splitlines()],
        )

    import asyncio

    asyncio.create_task(handle_runtime_warning(full_msg))


def trigger_max_hits(*, handler: TriggerCallable, max_hits: int = 1) -> TriggerCallable:
    __ensure_async(handler)

    async def trigger_max_hits_wrapper(
        session_id: SessionId, trigger_id: TriggerId, line: str, groups: list[str]
    ):
        # Call the wrapped handler.
        await handler(session_id, trigger_id, line, groups)

        trigger = await mudpuppy_core.get_trigger(session_id, trigger_id)
        if trigger is None:
            logging.warning(f"trigger_max_hits: trigger ID {trigger_id} missing")
            return

        if trigger.config.hit_count >= max_hits:
            logging.debug(
                f"trigger_max_hits: '{trigger.config.name}' ({trigger_id}) hit max limit of {max_hits}"
            )
            await mudpuppy_core.disable_trigger(session_id, trigger_id)

    return trigger_max_hits_wrapper


def alias_max_hits(*, handler: AliasCallable, max_hits: int = 1) -> AliasCallable:
    __ensure_async(handler)

    async def alias_max_hits_wrapper(
        session_id: SessionId, alias_id: AliasId, line: str, groups: list[str]
    ):
        # Call the wrapped handler.
        await handler(session_id, alias_id, line, groups)

        alias = await mudpuppy_core.get_alias(session_id, alias_id)
        if alias is None:
            logging.warning(f"alias_max_hits: alias ID {alias_id} missing")
            return

        if alias.config.hit_count >= max_hits:
            logging.debug(
                f"alias_max_hits: '{alias.config.name}' ({alias_id}) hit max limit of {max_hits}"
            )
            await mudpuppy_core.disable_alias(session_id, alias_id)

    return alias_max_hits_wrapper


# Set up custom warning handling
warnings.showwarning = custom_showwarning
