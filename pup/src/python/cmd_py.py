import logging
import inspect
import contextlib
import io
import asyncio

import pup
from pup import Session, OutputItem

import history


async def run_task(code: str, session: Session):
    tab = await session.tab()
    tab_title = await tab.title()
    tab_shortcuts = await tab.shortcuts()
    layout = await tab.layout()
    input = await session.input()
    connection_info = await session.connection_info()
    buffers = await session.get_buffers()
    prompt = session.prompt()
    prompt_line = await prompt.get()
    prompt_mode = await prompt.mode()
    triggers = session.triggers()
    trigger_list = await triggers.get()
    aliases = session.aliases()
    alias_list = await aliases.get()
    config = await pup.config()
    sessions_list = await pup.sessions()
    global_shortcuts = await pup.global_shortcuts()
    tabs_list = await pup.tabs()

    eval_globals = globals().copy()
    eval_globals.update(
        {
            "history": history,
            "session": session,
            "info": connection_info,
            "tab": tab,
            "tab_title": tab_title,
            "tab_shortcuts": tab_shortcuts,
            "layout": layout,
            "input": input,
            "buffers": buffers,
            "prompt": prompt,
            "prompt_line": prompt_line,
            "prompt_mode": prompt_mode,
            "triggers": triggers,
            "trigger_list": trigger_list,
            "aliases": aliases,
            "alias_list": alias_list,
            "config": config,
            "pup": pup,
            "sessions_list": sessions_list,
            "global_shortcuts": global_shortcuts,
            "tabs_list": tabs_list,
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

        for line in stdout_buff.getvalue().splitlines():
            session.output(OutputItem.command_result(line))

    except Exception as e:
        session.output(OutputItem.failed_command_result(f"Error running code: {e}"))


async def run(code: str, session: Session):
    asyncio.create_task(run_task(code, session))


pup.add_slash_command("py", run_task)
