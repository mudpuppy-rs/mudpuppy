# Output

Mudpuppy displays outputs per-MUD in a special output buffer. Your Python code
can add items to be displayed through the `mudpuppy_core` API, or for simple
debugging, using `print()`.

Presently only the **low-level** API/types are available. In the future there
will be helpers to make this less painful :-)

[logging]: ../logging.md

## Debug Output

For simple debug output you can use `print()`. It will convert each line of what
would have been written to stdout into `OutputItem.debug()` instances that get
added to the currently active session. If called when there is no active
session, nothing will be displayed - prefer `logging` for this use-case.

You can also use `print()` from `/py` but you must carefully escape the input:
```
/py print(\"this is a test\\nhello!\")
```

## Adding Output

Other kinds of output can be added using `mudpuppy_core.add_output()` and
providing both the `SessionId` to add the output to, and an `OutputItem` to add.
Remember this is an async operation so you'll need to `await`!

```python
from mudpuppy_core import mudpuppy_core, OutputItem

await mudpuppy_core.add_output(
    sesh_id, OutputItem.command_result("This was a test")
)
```

## Output Item Types

There are several `OutputItem` types you can construct to use with
`add_output()`:

1. `OutputItem.command_result(msg)` - for constructing output that should be
   rendered as separate from game output. Generally this is used when the 
   operation being described was successful. 

2. `OutputItem.failed_command_result(msg)` - similar to above, but for
   operations that failed and should be displayed as an error result.

3. `OutputItem.mud_line(line)` - for displaying output as if it came from the MUD. You'll
   need to construct a `MudLine` as the argument. E.g.:

```python
from mudpuppy_core import MudLine, OutputItem

item = output_item.mud_line(MudLine(b"Some fake MUD output!"))
```

There is also `OutputItem.prompt()` and `OutputItem.held_prompt()` that take
a `MudLine` but treat it as a prompt, or held prompt.

4. `OutputItem.input(line)` - for displaying input as if it came from the user.
   You'll need to construct a `InputLine` for the argument. E.g.:

```python
from mudpuppy_core import InputLine, OutputItem

line = InputLine("some fake input!")
line.original = "FAKE!"
item = output_item.input(line)
```

5. `OutputItem.debug(msg)` - for displaying debug information.
