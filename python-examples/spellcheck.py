import logging

from mudpuppy import on_event
from mudpuppy_core import mudpuppy_core, Event, EventType, Input, EchoState
from cformat import cformat
from commands import commands  # type: ignore  # TODO(XXX): .pyi for commands


@on_event(EventType.KeyPress)
async def markup_input(event: Event):
    """
    An example KeyPress handler that adds markup to highlight input based on
    spellchecking and slash command status.
    """
    assert isinstance(event, Event.KeyPress)

    i = await mudpuppy_core.input(event.id)
    if i.telnet_echo() == EchoState.Password:
        i.clear_markup()
        return  # Don't try to spellcheck/highlight masked password entry!

    to_be_sent = i.value().sent
    if len(to_be_sent) <= 1:
        return

    spellcheck_input(event.id, i, to_be_sent.split())


def highlight_cmd(session_id: int, i: Input, cmd: str):
    # Check if the command is valid.
    valid = commands[session_id].get(cmd[1:])

    # Add the appropriate markup to the command part.
    i.add_markup(0, cformat("<bold><green>") if valid else cformat("<bold><red>"))
    i.add_markup(len(cmd), cformat("<reset>"))


def spellcheck_input(session_id: int, i: Input, parts: list[str]):
    i.clear_markup()

    start = 0
    for part in parts:
        end = start + len(part)
        # remove leading/trailing punctuation for a dictionary lookup.
        clean_part = part.strip(".,!?;:'\"`[]{}()\\/<>~!@#$%^&*_-+=").lower()
        if start == 0 and part.startswith("/"):
            # If the first part is a slash command, highlight it specially instead of spellchecking.
            highlight_cmd(session_id, i, part)
        elif dictionary is not None and not dictionary.lookup(clean_part):
            # logging.debug(f"misspelled word: {clean_part} ({start}, {end})")
            i.add_markup(start, cformat("<bold><red>"))
            i.add_markup(end, cformat("<reset>"))

        start = end + 1  # Offset by 1 to account for the space between words.


try:
    from spylls.hunspell import Dictionary  # type: ignore

    # TODO(XXX): support other languages
    dictionary = Dictionary.from_files("en_US")
except ImportError:
    logging.warning("spylls is not in the PYTHONPATH. Spellchecking will be disabled.")
    logging.warning("perhaps you need to 'pip install spylls'?")
    dictionary = None
