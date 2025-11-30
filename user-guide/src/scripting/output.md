# Output

Mudpuppy displays outputs per-MUD in a special output buffer. Your Python code
can add items to be displayed through the `mudpuppy_core` API, or for simple
debugging, using `print()`.

Presently only the **low-level** API/types are available. In the future there
will be helpers to make this less painful :-)

See the API reference for [OutputItem] as well as the [add_output()] and
[add_outputs()] functions for more information.

You may also want to use [cformat] for colouring output you create.

[logging]: ../logging.md
[OutputItem]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#OutputItem
[add_output()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#MudpuppyCore.add_output
[add_outputs()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#MudpuppyCore.add_outputs
[cformat]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/cformat.html#cformat

## Debug Output

For simple debug output you can use `print()`. It will convert each line of what
would have been written to stdout into [OutputItem.debug()] instances that get
added to the currently active session. If called when there is no active
session, nothing will be displayed - prefer `logging` for this use-case.

You can also use `print()` from `/py` but you must carefully escape the input:
```
/py print(\"this is a test\\nhello!\")
```

[OutputItem.debug()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#OutputItem.debug

## Adding Output

Other kinds of output can be added using
[mudpuppy_core.add_output()][add_output()] and
providing both the session ID to add the output to, and an [OutputItem] to add.
Remember this is an async operation so you'll need to `await`!

```python
from mudpuppy_core import mudpuppy_core, OutputItem

await mudpuppy_core.add_output(
    sesh_id, OutputItem.command_result("This was a test")
)
```

## Output Item Types

There are several [OutputItem] types you can construct to use with
[add_output()]:

1. [OutputItem.command_result()] - for constructing output that should be
   rendered as separate from game output. Generally this is used when the 
   operation being described was successful. 

2. [OutputItem.failed_command_result()] - similar to above, but for
   operations that failed and should be displayed as an error result.

3. [OutputItem.mud()] - for displaying output as if it came from the MUD. You'll
   need to construct a [MudLine] as the argument. E.g.:

```python
from mudpuppy_core import MudLine, OutputItem

item = output_item.mud_line(MudLine(b"Some fake MUD output!"))
```

[OutputItem.command_result()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#OutputItem.command_result
[OutputItem.failed_command_result()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#OutputItem.failed_command_result
[OutputItem.mud()]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#OutputItem.mud
[MudLine]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#MudLine

There is also `OutputItem.prompt()` and `OutputItem.held_prompt()` that take
a [MudLine] but treat it as a prompt, or held prompt.

4. `OutputItem.input(line)` - for displaying input as if it came from the user.
   You'll need to construct a [InputLine] for the argument. E.g.:

```python
from mudpuppy_core import InputLine, OutputItem

line = InputLine("some fake input!")
line.original = "FAKE!"
item = output_item.input(line)
```

[InputLine]: https://mudpuppy-rs.github.io/mudpuppy/api-docs/mudpuppy_core.html#InputLine

5. [OutputItem.debug()] - for displaying debug information.
