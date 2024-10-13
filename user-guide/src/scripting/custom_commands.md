# Custom Commands

Mudpuppy comes with a number of [built-in commands][commands]. You can also have
your Python scripts add new custom commands. This is an attractive alternative
to [aliases] when you want to support parse command-line arguments and flags.

The default command prefix is `/` but can be altered in configuration.

[commands]: ../commands.md
[aliases]: aliases.md

## Simple command

Commands are created by extending the `Command` class and registering the
command for a specific session with `commands.add_command()`.

Your command's `__init__()` should call the `super().__init__` with:

1. The command's name.
2. The session ID.
2. The command's main func.
3. A description of the command.

Here's a simple command that when `/simple` is run, will log a message.

```python
import logging
from mudpuppy_core import SessionId, Event
from commands import Command, add_command

@on_new_session()
async def setup(event: Event):
    add_command(event.id, SimpleCmd(event.id))


class SimpleCmd(Command):
    def __init__(self, session: SessionId):
        super().__init__("simple", session, self.simple, "A simple command example")

    async def simple(self, sesh_id: SessionId, _args: Namespace):
        logging.debug("Hello world!")

```

## Command-line args

To define a command that takes command-line args and has sub-commands look at
existing examples of built-in commands like [/trigger].

[/trigger]: https://github.com/mudpuppy-rs/mudpuppy/blob/main/mudpuppy/python/cmd_trigger.py
