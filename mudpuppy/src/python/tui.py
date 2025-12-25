import asyncio
import builtins
import warnings

import pup


def custom_showwarning(message, category, filename, lineno, _file=None, _line=None):
    # Send warning to error channel instead of printing to stdout (which corrupts TUI)
    full_msg = f"{filename}:{lineno}: {category.__name__}: {message}"
    pup.show_error(full_msg)


def custom_exception_handler(_loop, context):
    # Handle asyncio exceptions (like unawaited coroutines) via error channel
    message = context.get("exception", context.get("message", "Unknown error"))
    error_msg = f"Asyncio error: {message}"

    # Also include task info if available
    if "task" in context:
        task = context["task"]
        error_msg = f"Asyncio error in {task.get_name()}: {message}"

    pup.show_error(error_msg)


builtins.print = pup.print
warnings.showwarning = custom_showwarning

# Set up asyncio exception handler for unawaited coroutines
try:
    loop = asyncio.get_event_loop()
    loop.set_exception_handler(custom_exception_handler)
except RuntimeError:
    # No event loop yet, will be set when loop is created
    pass
