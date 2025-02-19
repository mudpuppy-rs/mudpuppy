# Timers

Timers run on a fixed interval. When the timer interval runs out, the timer
callback is invoked and then the timer is reset to wait for another interval.

* Timers are great for running an action on a regular cadence, like sending
  a "save" command every 15 minutes.

* Timers can be configured to only run a certain number of times. This can be
  helpful for something like running a "heal" command 3 times, with a 10 second
  wait between them.

Search the [API documentation] for [Timer][timer-search] to learn more.

[API documentation]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/index.html
[timer-search]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html?search=Timer

## Basic global timer

To make a basic timer that runs every 2 minutes, 10 seconds you can use the
[mudpuppy module]'s [@timer] decorator.

Since global timers run without being tied to a specific MUD they are provided
the currently focused active [SessionID] (if there is one!) as an argument:

```python
@timer(seconds=10, minutes=2)
async def party(timer_id: int, session_id: Optional[int]):
    logging.debug(f"2m10s timer fired: {timer_id}!")
    if session_id is not None:
        await mudpuppy_core.send_line(session_id, "say PARTY TIME!!!")
```

If you want to customize the name of the timer, provide a `name="Custom Name"`
argument to the [@timer] decorator. Otherwise, the name of the decorated function
is used.

[mudpuppy module]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy.html
[@timer]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy.html#timer

## Max ticks

Here's an example of a timer that's only defined when you connect to a MUD
named "Dune", and that only runs 3 times total (with a 10s wait between each
run).

```python
@timer(mud_name="Dune", seconds=10, max_ticks=3)
async def heal_timer(_timer_id: int, session_id: int):
    await mudpuppy_core.send_line(session_id, "heal")
```

Like [aliases] and [triggers] you can also pass a list of names to the [@timer]
decorator's `mud_name` parameter, like `mud_name=["Dune", "OtherMUD"]`.

[aliases]: aliases.md
[triggers]: triggers.md
