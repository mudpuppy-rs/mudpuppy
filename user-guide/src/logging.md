# Logging

Mudpuppy can log at a variety of different verbosity levels. This is a very
helpful mechanism when you're troubleshooting scripts, or trying to learn how
Mudpuppy works.

## Log location

This depends on your OS, but will generally be where applications keep their
non-configuration data:

| OS      | Logfile                                                               |
|---------|-----------------------------------------------------------------------|
| Linux   | `$HOME/.local/share/mudpuppy/mudpuppy.log`                            |
| MacOS   | `/Users/$USERNAME/Library/Application Support/mudpuppy/mudpuppy.log`  |
| Windows | `C:\Users\$USER\AppData\Roaming\mudpuppy\mudpuppy.log`                |

You can also find this directory from within Mudpuppy by running:

```
/py mudpuppy_core.data_dir()
```

Or from a Python script with:

```python
from mudpuppy_core import mudpuppy_core
path = mudpuppy_core.data_dir()
```

### Customizing config/data directories

You can also set the `MUDPUPPY_CONFIG` and `MUDPUPPY_DATA` environment variables
to customize the config and data dir that Mudpuppy will use. For example, on
a UNIX-like operating system you could run:

```bash
MUDPUPPY_CONFIG=$HOME/mudpuppy-test/config MUDPUPPY_DATA=$HOME/mudpuppy-test/data mudpuppy
```

## Log Level

By default Mudpuppy logs at the "info" level. You can change the log level by
setting an environment variable, or using the `--log-level` command line
argument:

```bash
# Via env var:
RUST_LOG=mudpuppy=trace mudpuppy

# Or, via the CLI:
mudpuppy --log-level=trace
```

The available log levels (in increasing level of verbosity/spam) are:

1. error
2. warn
3. info
4. debug
5. trace

# Python logging

Your Python code can use the normal [logging] library and the log information
will be sent to the same place as Mudpuppy's own logs.

```python
import logging
logging.warning("hello from Python code!")
```

[logging]: https://docs.python.org/3/library/logging.html
