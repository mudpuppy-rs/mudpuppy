# Aliases

Aliases match **input** you send to the MUD. When the input matches an alias'
pattern, the alias callback will be executed.

* Aliases can be used for something as simple as expanding "e" to "east", or for
more complex actions like making an HTTP request.

* Aliases can be added so that they're only available for MUDs with a certain
name, or so that they're available for all MUDs you connect to.

* The line that matched the alias, as well as any regexp groups in the pattern are
provided to the alias callback function alongside the session ID of the MUD.

Search the [API documentation] for [Alias][alias-search] to learn more.

[API documentation]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/index.html
[alias-search]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html?search=Alias

## Basic global alias

To make a basic alias that would expand the input "e" to "east" for all MUDs you
can use the [@alias] decorator. For example, adding this to a mudpuppy Python
script:

```python
from mudpuppy import alias
from mudpuppy_core import mudpuppy_core

@alias(pattern="^e$", expansion="east")
async def quick_east(_session_id: int, _alias_id: int, _line: str, _groups):
    pass
```

Providing `expansion` is a short-cut for "expanding" the input that was matched
by the pattern, by replacing it with the `expansion` value. The above example is
equivalent to using [send_line()] directly:

```python
@alias(pattern="^e$", name="Quick East")
async def quick_east(session_id: int, _alias_id: int, _line: str, _groups):
    await mudpuppy_core.send_line(session_id, "east")
```

If you want to customize the name of the alias, provide a `name="Custom Name"` 
argument to the [@alias] decorator. Otherwise, the name of the decorated function
is used.

[@alias]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy.html#alias
[send_line()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#MudpuppyCore.send_line

## Per-MUD alias

Here's an example of an alias that's only defined when you connect to a MUD
named "Dune".

It also demonstrates how to use a match group and the convenience
of async aliases for doing things like "waiting a little bit" without blocking
the client, or needing to use a separate timer.

It will match input like "kill soldier", pass the command through to the game,
and then also wait 5 seconds before issuing the command "headbutt soldier".

```python
import logging
import asyncio
from mudpuppy import alias
from mudpuppy_core import mudpuppy_core


@alias(mud_name="Dune", pattern="^kill (.*)$")
async def kill_headbutt(session_id: int, _alias_id: int, line: str, groups):
    # Send through the original line so that we actually start combat in-game
    # with the 'kill' command.
    await mudpuppy_core.send_line(session_id, line)

    # Wait for a little bit, and then give them a headbutt!
    target = groups[0]
    logging.info(f"building up momentum for a headbutt attack on {target}")

    await asyncio.sleep(5)
    await mudpuppy_core.send_line(session_id, f"headbutt {target}")
```

If you wanted to have this alias also available on MUDs named "DevDune" and
"Dune (Alt)" the `mud_name` can be changed to a list:

```python
@alias(mud_name=["Dune","DevDune","Dune (alt)"], pattern="^kill (.*)$")
async def kill_headbutt(session_id: int, _alias_id: int, line: str, groups):
    ...
```

## Alias info

You can use the alias ID passed to the alias handler to access information
about the alias, like how many times it has matched. See the [get_alias()] and
[AliasConfig] API references for more information.

This can be used for things like disabling an alias after a certain number of
usages:

```python
from mudpuppy import alias
from mudpuppy_core import mudpuppy_core, OutputItem

@alias(pattern="^backstab$")
async def backstab(session_id: int, alias_id: int, line: str, _groups):
    alias_info = await mudpuppy_core.get_alias(session_id, alias_id)

    # Too many backstabs this session!
    hits = alias_info.config.hit_count
    if hits > 10:
        msg = f"backstabbed {hits} times already. Ignoring cmd."
        logging.info(msg)
        await mudpuppy_core.add_output(
            session_id, OutputItem.failed_command_result(msg)
        )
        return

    # Do the backstab
    await mudpuppy_core.send_line(session_id, line)
```

[get_alias()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#MudpuppyCore.get_alias
[AliasConfig]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#AliasConfig
