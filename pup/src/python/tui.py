import builtins
import warnings

import pup


def custom_showwarning(message, category, filename, lineno, _file=None, _line=None):
    # Log the warning to the mudpuppy log file.
    full_msg = f"{filename}:{lineno}: {category.__name__}: {message}"
    logging.warning(full_msg)

    for line in full_msg.splitlines():
        print(line)


# TODO(XXX): only do this setup for TUI mode.
builtins.print = pup.print
warnings.showwarning = custom_showwarning
