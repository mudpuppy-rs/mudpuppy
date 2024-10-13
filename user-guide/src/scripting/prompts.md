# Prompts

Mudpuppy attempts to detect what is/isn't a prompt in a few ways (listed in
order of how reliable they are):

1. Negotiating support for the telnet "EOR" option, and expecting prompts to be
   terminated with EOR.
2. Seeing lines that end with telnet "GA", and assuming they are prompts.
3. Seeing lines that end without `\r\n`, after a short timeout expires to ensure
   it wasn't a partial line.

It is not presently possible to set the prompt handling mode manually, it is
determined based on whether the MUD supports the telnet options mentioned above.
Similarlyh it isn't presently possible to change the prompt flushing timeout for
unterminated prompt mode. In the future this will be more flexible.

## Prompt Event Handlers

To write a handler that fires for every prompt line for any MUD, write:

```python
from mudpuppy_core import EventType, Event, SessionId, MudLine
from mudpuppy import on_event

@on_event(EventType.Prompt)
async def prompt_handler(event: Event):
    session_id: SessionId = event.id
    prompt_line: MudLine = event.prompt
    logging.debug(f"session {session_id} got prompt line {prompt_line}")
```

Similar to [aliases], [triggers], and [timers] it's also possible to write
a handler that only fires for prompt events for specifically named MUDs.

```python
@on_event(EventType.Prompt)
async def prompt_handler(event: Event):
    session_id: SessionId = event.id
    prompt_line: MudLine = event.prompt
    logging.debug(f"session {session_id} got prompt line {prompt_line}")
```

Similar to [aliases], [triggers], and [timers] it's also possible to write
a handler that only fires for prompt events for specifically named MUDs.

```python
@on_mud_event(["Dune", "OtherMud"], EventType.Prompt)
async def prompt_handler(event: Event):
    ...
```


## Prompt Triggers

`MudLine`'s that are detected as a prompt have the `prompt` field set to `True`.
See [triggers] for more information on how to write triggers that only match
prompt lines.

This is genereally more useful if you want to only match certain prompt
patterns, or to gag prompts.

[triggers]: triggers.md
