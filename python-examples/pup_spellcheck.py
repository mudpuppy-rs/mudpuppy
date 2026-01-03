import logging
import re

from pup import (
    Session,
    EventType,
    Event,
    InputLine,
    OutputItem,
    EchoState,
    Markup,
)


ANSI_CODES = {
    "reset": "0",
    "bold": "1",
    "inverted": "7",
    "black": "30",
    "red": "31",
    "green": "32",
    "yellow": "33",
    "blue": "34",
    "magenta": "35",
    "cyan": "36",
    "white": "37",
    "bg_black": "40",
    "bg_red": "41",
    "bg_green": "42",
    "bg_yellow": "43",
    "bg_blue": "44",
    "bg_magenta": "45",
    "bg_cyan": "46",
    "bg_white": "47",
}


def cformat(text: str) -> str:
    def ansi_code(token):
        return f"\033[{ANSI_CODES[token]}m" if token in ANSI_CODES else f"<{token}>"

    def replace_tokens(match):
        token = match.group(1)
        return ansi_code(token)

    pattern = re.compile(r"<(.*?)>")
    return pattern.sub(replace_tokens, text)


class Spellchecker:
    def __init__(self, sesh: Session, dictionary: str = "en_US"):
        self.dictionary_name = dictionary
        self.dictionary = None

        try:
            from spylls.hunspell import Dictionary  # type: ignore

            self.dictionary = Dictionary.from_files(dictionary)
            sesh.add_event_handler(EventType.InputChanged, self.input_changed)
        except ImportError:
            msg = """
            'spylls' is not in the PYTHONPATH. Spellchecking will be disabled. Perhaps you need to 'pip install spylls'?
            """
            logging.warning(msg)
            sesh.output(OutputItem.failed_command_result(msg))
        except FileNotFoundError:
            msg = f"dictionary '{dictionary}' could not be loaded"  # TODO(XXX): advice.
            logging.warning(msg)
            sesh.output(OutputItem.failed_command_result(msg))

    async def input_changed(self, sesh: Session, ev: Event):
        assert isinstance(ev, Event.InputChanged)

        input = ev.input
        # InputChanged events always have InputLine, not MudLine
        assert isinstance(ev.line, InputLine)
        line = ev.line

        if input.echo() == EchoState.Password:
            return

        markup = input.markup()
        markup.clear()

        await self.spellcheck_input(sesh, line, markup)

    async def spellcheck_input(self, sesh: Session, line: InputLine, markup: Markup):
        if self.dictionary is None:
            logging.warning("dictionary was None")
            return
        parts = line.sent.split()

        start = 0
        for part in parts:
            end = start + len(part)
            # remove leading/trailing punctuation for a dictionary lookup.
            clean_part = part.strip(".,!?;:'\"`[]{}()\\/<>~!@#$%^&*_-+=").lower()
            if start == 0 and part.startswith("/"):
                # If the first part is a slash command, highlight it specially instead of spellchecking.
                await self.highlight_cmd(sesh, markup, part)
            elif not self.dictionary.lookup(clean_part):
                # logging.debug(f"misspelled word: {clean_part} ({start}, {end})")
                markup.add(start, cformat("<bold><red>"))
                markup.add(end, cformat("<reset>"))

            start = end + 1  # Offset by 1 to account for the space between words.

    async def highlight_cmd(self, sesh: Session, markup: Markup, cmd: str):
        # Check if the command is valid.
        exists = await sesh.slash_command_exists(cmd[1:])

        # Add the appropriate markup to the command part.
        markup.add(0, cformat("<bold><green>") if exists else cformat("<bold><red>"))
        markup.add(len(cmd), cformat("<reset>"))
