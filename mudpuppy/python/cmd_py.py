import logging
from argparse import Namespace, REMAINDER
import inspect
import contextlib
import io

from commands import Command, add_command
from mudpuppy_core import Event, OutputItem, mudpuppy_core

from mudpuppy import on_new_session


class Python(Command):
    def __init__(self, session: int):
        super().__init__(
            "python", session, self.run, "Run Python code", aliases=["py", "eval"]
        )
        self.parser.add_argument(
            "code", nargs=REMAINDER, help="Code to execute or evaluate"
        )

        import commands
        import history
        import mudpuppy
        from cformat import cformat

        self.eval_globals = globals().copy()
        self.eval_globals.update(
            {
                "history": history,
                "commands": commands,
                "mudpuppy": mudpuppy,
                "config": mudpuppy_core.config(),
                "session": session,
                "session_info": None,
                "cformat": cformat,
            }
        )

    async def invoke(self, sesh_id: int, args: str):
        processed_args = args.replace(r'"', r"\"")
        processed_args = processed_args.replace(r"'", r"\'")
        await super().invoke(sesh_id, processed_args)

    async def run(self, sesh_id: int, args: Namespace):
        code = " ".join(args.code) if args.code else ""
        logging.debug(
            f"pycmd: eval: {code}",
        )
        try:
            session_info = await mudpuppy_core.session_info(sesh_id)
            self.eval_globals["session_info"] = session_info

            with contextlib.redirect_stdout(io.StringIO()) as stdout_buff:
                try:
                    result = eval(code, self.eval_globals)
                    if inspect.isawaitable(result):
                        result = await result
                    if result is not None:
                        await mudpuppy_core.add_output(
                            sesh_id, OutputItem.command_result(repr(result))
                        )
                except SyntaxError:
                    exec(code, self.eval_globals)

            stdout = [
                OutputItem.command_result(line)
                for line in stdout_buff.getvalue().splitlines()
            ]
            await mudpuppy_core.add_outputs(sesh_id, stdout)

        except Exception as e:
            await mudpuppy_core.add_output(
                sesh_id, OutputItem.failed_command_result(f"Error running code: {e}")
            )


@on_new_session()
async def setup(event: Event):
    assert isinstance(event, Event.NewSession)
    add_command(event.id, Python(event.id))
