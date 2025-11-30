import logging
import inspect
import contextlib
import io

import pup
from pup import Session, OutputItem

from pup_events import command

import history


# TODO(XXX): hacky. Also doesn't preserve globals() between invocations like desired to make eval() meaningful.
@command("py")
async def run_task(code: str, session: Session):
    logging.debug(f"setting up globals for {session}")

    tab = await session.tab()
    eval_globals = globals().copy()
    eval_globals.update(
        {
            "history": history,
            "session": session,
            "info": await session.connection_info(),
            "tab": tab,
            "tab_title": await tab.title(),
            "tab_shortcuts": await tab.shortcuts(),
            "layout": await tab.layout(),
            "input": await session.input(),
            "buffers": await session.get_buffers(),
            "prompt": session.prompt(),
            "prompt_line": await session.prompt().get(),
            "prompt_mode": await session.prompt().mode(),
            "triggers": session.triggers(),
            "trigger_list": await session.triggers().get(),
            "aliases": session.aliases(),
            "alias_list": await session.aliases().get(),
            "config": await pup.config(),
            "pup": pup,
            "sessions_list": await pup.sessions(),
            "global_shortcuts": await pup.global_shortcuts(),
            "tabs_list": await pup.tabs(),
        }
    )

    logging.debug(
        f"pycmd: eval: {code}",
    )
    try:
        with contextlib.redirect_stdout(io.StringIO()) as stdout_buff:
            try:
                result = eval(code, eval_globals)
                if inspect.isawaitable(result):
                    result = await result
                if result is not None:
                    session.output(OutputItem.command_result(repr(result)))
            except SyntaxError:
                exec(code, eval_globals)

        session.output(
            [
                OutputItem.command_result(line)
                for line in stdout_buff.getvalue().splitlines()
            ]
        )

    except Exception as e:
        session.output(OutputItem.failed_command_result(f"Error running code: {e}"))


logging.debug("module loaded")
