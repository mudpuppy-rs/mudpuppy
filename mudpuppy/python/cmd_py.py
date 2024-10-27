import logging
from argparse import Namespace

from commands import Command, add_command
from mudpuppy_core import Event, OutputItem, SessionId, mudpuppy_core

from mudpuppy import on_new_session


class Python(Command):
    def __init__(self, session: SessionId):
        super().__init__(
            "python", session, self.run, "Run Python code", aliases=["py", "eval"]
        )
        self.parser.add_argument("code", nargs="+", help="Code to execute or evaluate")

    async def run(self, sesh_id: SessionId, args: Namespace):
        code = " ".join(args.code if args.code is not None else [])
        logging.debug(
            f"pycmd: eval: {code}",
        )
        try:
            import commands
            import history

            import mudpuppy

            session_info = await mudpuppy_core.session_info(sesh_id)

            eval_globals = globals().copy()
            eval_globals.update(
                {
                    "history": history,
                    "commands": commands,
                    "mudpuppy": mudpuppy,
                    "config": mudpuppy_core.config(),
                    "session": sesh_id,
                    "session_info": session_info,
                }
            )
            result = eval(code, eval_globals)
            await mudpuppy_core.add_output(
                sesh_id, OutputItem.command_result(repr(result))
            )
        except Exception as e:
            await mudpuppy_core.add_output(
                sesh_id, OutputItem.failed_command_result(f"Error running code: {e}")
            )


@on_new_session()
async def setup(event: Event):
    assert isinstance(event, Event.NewSession)
    add_command(event.id, Python(event.id))
