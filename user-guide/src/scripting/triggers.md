# Triggers

Triggers match a line of **output** sent by the MUD. When the output matches an
trigger's pattern, the trigger callback will be executed.

* Triggers can be used for something as simple as sending "light torch" whenever
the game sends the line "It is too dark to see." or for more complex actions
like automatically making an HTTP request to look up the name of an item in
a database when you see it on the ground.

* Triggers can be added so that they're only available for MUDs with a certain
name, or so that they're available for all MUDs you connect to.

* The line that matched the trigger, as well as any regexp groups in the pattern are
provided to the trigger callback function alongside the session ID of the MUD.

<div class="warning">
Note that all triggers are matched a line at a time. Multi-line triggers are not
yet supported.
</div>

Search the [API documentation] for [Trigger][trigger-search] to learn more.

[API documentation]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/index.html
[trigger-search]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html?search=Trigger

## Basic global trigger

To make a basic trigger that would match the line "Your ship has landed." and
automatically send "enter ship" you can use the [mudpuppy module]'s [@trigger]
decorator.

```python
@trigger(
    pattern=r"^Your ship has landed\.$"
    expansion="enter ship",
)
async def quick_ship(_session_id: int, _trigger_id: int, _line: str, _groups):
    pass
```

If you want to customize the name of the trigger, provide a `name="Custom Name"`
argument to the [@trigger] decorator. Otherwise, the name of the decorated function
is used.

Providing expansion is a short-cut for "expanding" the input that was matched by
the pattern, by replacing it with the expansion value. The above example is
equivalent to awaiting [send_line()] directly:

```python
@trigger(
    pattern=r"^Your ship has landed\.$"
)
async def quick_ship(session_id: int, _trigger_id: int, _line: str, _groups):
    await mudpuppy_core.send_line(session_id, "enter ship")
```

[mudpuppy module]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy.html#trigger
[@trigger]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy.html
[send_line()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#MudpuppyCore.send_line

## Per-MUD triggers

Like [aliases](aliases.md) you can define triggers for only certain MUDs by
providing a `mud_name` string, or list of strings as an argument to the
[@trigger] decorator:

```python
@trigger(
    mud_name=["Dune", "DevDune"],
    pattern=r"^Your ship has landed\.$",
    expansion="enter ship",
)
async def quick_ship(_session_id: int, _trigger_id: int, _line: str, _groups):
    pass
)
```

## Output gags

If you want to silence, supress or "gag" lines of output you can write a trigger
that matches the lines you wish to gag, setting `gag=True` in the [@trigger]
decorator:

```python
@trigger(
    pattern=r"^(?:Autosave)|(?:Your character has been saved safely)\.$",
    gag=True
)
async def quiet_saves(_session_id: int, _trigger_id: int, _line: str, _groups):
    pass
)
```

## Matching prompt lines

You can also create triggers that only match prompt lines by specifying
`prompt=True` in the [@trigger] decorator. This can also be combined with
`gag=True` to gag matched prompts.

See [prompts] for more information on how prompts are detected.

```python
import logging

@trigger(
    prompt=True,
    gag=True,
    pattern=r"(?:Enter your username: )|(?:Password: )"
)
async def gag_login(_session_id: int, _trigger_id: int, line: str, _groups: Any):
    logging.debug(f"gagged login prompt: {line}")
```

[prompts]: prompts.md

## Matching ANSI

By default triggers are created with `strip_ansi=True`. Lines of text will have
any ANSI colour codes removed before evaluating the trigger pattern.

If you want to write a trigger that matches on ANSI you need to specify
`strip_ansi=False` in the [@trigger] decorator:

```python
import logging

@trigger(
    strip_ansi=False,
    pattern=r"\033\[[\d]+;1m(.*)\033\[0m",
)
async def quiet_saves(_session_id: int, trigger_id: int, _line: str, groups):
    logging.info(f"quiet_saves({trigger_id}) matched bold text: {groups[0]}")
)
```
