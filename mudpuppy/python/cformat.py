import logging
import re

__all__ = ["cformat", "ANSI_CODES"]

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
    """
    Return `text` but with tokens of the form `<colour>` replaced with ANSI escape codes.

    See `ANSI_CODES` for available colour tokens.

    Example:
    ```python
    from cformat import cformat
    msg = cformat("<red>red text<reset> normal text")
    ````
    """

    def ansi_code(token):
        return f"\033[{ANSI_CODES[token]}m" if token in ANSI_CODES else f"<{token}>"

    def replace_tokens(match):
        token = match.group(1)
        return ansi_code(token)

    pattern = re.compile(r"<(.*?)>")
    return pattern.sub(replace_tokens, text)


logging.debug("cformat module loaded")
