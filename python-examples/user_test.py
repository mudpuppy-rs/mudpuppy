import asyncio
import logging
from random import randint
from typing import Optional

from cformat import cformat
from mudpuppy_core import (
    AliasId,
    Event,
    EventType,
    MudLine,
    OutputItem,
    SessionId,
    TimerId,
    TriggerId,
    event_handlers,
    mudpuppy_core,
)

from mudpuppy import (
    alias,
    highlight,
    on_connected,
    on_disconnected,
    on_event,
    on_mud_disconnected,
    on_mud_event,
    on_new_session,
    timer,
    trigger,
    unload_handlers,
)

logging.debug("User Python Loaded!")


@on_new_session()
async def new_session(event: Event):
    logging.debug(f"new_session: {event}")


@on_connected()
async def mud_connected(event: Event):
    logging.debug(f"{event}")


@on_disconnected()
async def mud_disconnected(event: Event):
    logging.debug(f"{event}")


@on_event(EventType.ConfigReloaded)
async def config_event(event: Event):
    logging.debug(f"{event}")


@on_event(EventType.Prompt)
async def prompt_handler(event: Event):
    logging.debug(f"got prompt event: {event}")


@trigger(
    mud_name=["Test (TLS)", "Test (Telnet)"],
    name="MultiMud",
    pattern="^You say: secret.",
    expansion="say sauce.",
)
async def multi_mud_test(
    _session_id: SessionId, _trigger_id: TriggerId, _line: str, _groups
):
    pass


@on_mud_event("Test (TLS)", EventType.Prompt)
async def test_prompt_handler(event: Event):
    logging.debug(f'test prompt is: "{str(event.prompt)}"')
    if str(event.prompt) == "Please enter your name: ":
        await mudpuppy_core.send_line(event.id, "sneak")
    elif str(event.prompt) == "Password: ":
        await mudpuppy_core.send_line(event.id, "ilovemath")


@trigger(
    mud_name="Test (TLS)",
    name="Narf fun",
    pattern="^You (say|exclaim|ask): (.*) narf.$",
    expansion="say N A R F!!!",
)
async def narf(session_id: SessionId, trigger_id: TriggerId, _line: str, groups):
    await mudpuppy_core.send_line(
        session_id, f"say don't much care about {groups[1]} tbh"
    )
    trig = await mudpuppy_core.get_trigger(session_id, trigger_id)
    await mudpuppy_core.send_line(
        session_id, f"say I've done this dance {trig.config.hit_count} times already"
    )


@trigger(name="Global test", pattern="^You say: test.$", expansion="say test PASSED")
async def test_trig(
    _session_id: SessionId, _trigger_id: TriggerId, _line: str, _groups
):
    pass


@highlight(name="IP highlight", pattern=r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})")
def ip_highlight(line: MudLine, groups):
    assert len(groups) == 1
    hilight = line.__str__().replace(
        groups[0], cformat(f"<bold><cyan>{groups[0]}<reset>")
    )
    line.set(hilight)
    return line


@alias(mud_name="Test (TLS)", pattern="^myip$", name="Check my IP address")
async def ip_alias(session_id: SessionId, _alias_id: AliasId, _line: str, _groups):
    try:
        import aiohttp
    except ImportError:
        await mudpuppy_core.add_output(
            session_id, OutputItem.failed_command_result("aiohttp not available")
        )
        return

    async with aiohttp.ClientSession() as session:
        async with session.get("https://www.icanhazip.com/") as response:
            body = await response.text()
            await mudpuppy_core.add_outputs(
                session_id,
                [
                    OutputItem.command_result(f"Status: {response.status}"),
                    OutputItem.command_result(f"Body: {body}"),
                ],
            )


@alias(pattern="^e$", name="Quick East", expansion="east")
async def quick_east(_session_id: SessionId, _alias_id: AliasId, _line: str, _groups):
    pass


@alias(mud_name="Test (TLS)", pattern="^yeet$", name="Yeet Alias")
async def yeet(session_id: SessionId, alias_id: AliasId, line: str, groups):
    logging.debug(
        f"session {session_id} alias {alias_id} match line: {line} groups: {groups}"
    )
    e = "E" * randint(1, 25)
    sleep = randint(2, 15)
    logging.debug(f"sleeping for {sleep} before yeet")
    await asyncio.sleep(sleep)
    await mudpuppy_core.send_line(
        session_id,
        f"say Y{e}T!",
    )


@on_mud_disconnected("Test (TLS)")
async def test_tls_disconnected(_event: Event):
    logging.debug("Test (TLS) disconnected! :(")


@timer(name="Global Test Timer", seconds=10, max_ticks=2)
async def global_woohoo(timer_id: TimerId, _session_id: Optional[SessionId]):
    logging.debug(f"global woohoo timer: {timer_id}!")
    config = await mudpuppy_core.get_timer(timer_id)
    logging.debug(f"global woohoo timer config: {config}")


@timer(mud_name="Test (TLS)", name="Test MUD Timer", seconds=5)
async def test_timer(timer_id: TimerId, session_id: Optional[SessionId]):
    assert session_id is not None
    await mudpuppy_core.add_output(
        session_id, OutputItem.command_result(f"[{timer_id}] Tick tock!")
    )
    await mudpuppy_core.send_line(session_id, "say tick tock")

    if randint(1, 3) == 1:
        await mudpuppy_core.add_output(
            session_id, OutputItem.command_result(f"[{timer_id}] Bye for now...")
        )
        await mudpuppy_core.cancel_timer(timer_id)


@alias(mud_name="Test (TLS)", pattern="^debug$", name="Debug Alias")
async def callback_debug(
    session_id: SessionId, _alias_id: AliasId, _line: str, _groups
):
    logging.debug(f"session {session_id} debug alias triggered")
    event_types = event_handlers.get_handler_events()
    for event_type in event_types:
        await mudpuppy_core.add_output(
            session_id, OutputItem.command_result(f"{event_type}:")
        )
        for handler, module in event_handlers.get_handlers(event_type):
            await mudpuppy_core.add_output(
                session_id, OutputItem.command_result(f"  {module} -> {handler}")
            )


def __reload__():
    logging.debug("\n\n\n\nUser Python About To Reload!\n\n\n\n")
    unload_handlers(__name__)
