import logging
import re

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


def tokens() -> list[str]:
    return list(ANSI_CODES.keys())


logging.debug("cformat module loaded")
