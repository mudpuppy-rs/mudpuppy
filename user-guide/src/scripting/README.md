# Scripting

Python scripts placed in the Mudpuppy config directory are automatically loaded
when Mudpuppy is started. This is the principle mechanism of scripting: putting
Python code that interacts with Mudpuppy through the `mudpuppy` and
`mudpuppy_core` packages in your config dir.

## Where is my config directory?

This depends on your OS, but will generally be where applications keep their
configuration:

| OS      | Config dir                                                |
|---------|-----------------------------------------------------------|
| Linux   | `$HOME/.config/mudpuppy/`                                 |
| MacOS   | `/Users/$USERNAME/Library/Application Support/mudpuppy/`  |
| Windows | `C:\Users\$USER\AppData\Roaming\mudpuppy\`                |

You can also find this directory from within Mudpuppy by running:

```
/py mudpuppy_core.config_dir()
```

Or from a Python script with:

```python
from mudpuppy_core import mudpuppy_core
path = mudpuppy_core.config_dir()
```

### Customizing config/data directories

You can also set the `MUDPUPPY_CONFIG` and `MUDPUPPY_DATA` environment variables
to customize the config and data dir that Mudpuppy will use. For example, on
a UNIX-like operating system you could run:

```bash
MUDPUPPY_CONFIG=$HOME/mudpuppy-test/config MUDPUPPY_DATA=$HOME/mudpuppy-test/data mudpuppy
```


## Mudpuppy packages

Your Python scripts can `import mudpuppy` and `import mudpuppy_core` to get
access to helpful interfaces for interacting with Mudpuppy and MUDs.

In general `mudpuppy_core` has low-level APIs, and helpful type definitions. On
the other hand `mudpuppy` has higher-level APIs, such as decorators for making
triggers/aliases/etc.

**TODO: API documentation auto-generated from the Python should be linked
here**.

## Async

Remember that Mudpuppy is **asynchronous**: most callbacks will need to be
defined `async`, and you will need to `await` most operations on the
`mudpuppy_core` interface.

```python
# Correct:
@trigger(
    mud_name="Dune",
    pattern="^Soldier died.$",
)
async def bloodgod(session_id: SessionId, _trigger_id: TriggerId, _line: str, _groups):
    await mudpuppy_core.send(session_id, "say blood for the blood god!")
```

It is an error to forget to define your functions with the `async` keyword, or
to forget to `await` async APIs like `mudpuppy_core.send()`:

```python
# INCORRECT (no 'async' on def, no 'await' for send):
@trigger(
    mud_name="Dune",
    pattern="^Soldier died.$",
)
def bloodgod(session_id: SessionId, _trigger_id: TriggerId, _line: str, _groups):
    mudpuppy_core.send(session_id, "say blood for the blood god!")
```

Mudpuppy will do its best to catch these errors for you, but it's helpful to
keep in mind.

