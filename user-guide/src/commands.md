# Commands

Mudpuppy has several built-in commands you can run from within the client. By
default the command prefix is "/". The choice of prefix can be changed in your
config file.

## `/status`

Shows the current connection status. Use `/status --verbose` for more
information.

## `/connect`

Connects the current session if it isn't already connected.

## `/disconnect`

Disconnects the current session if it isn't already disconnected.

## `/quit`

Exits Mudpuppy.

## `/reload`

Reloads user python scripts. Scripts can define a handler to be called before
reloading occurs if any clean-up needs to be done:

```python
# Called before /reload completes and the module is re-imported.
def __reload__():
    pass
```

## `/alias`, `/trigger`, `/timer`

These commands allow creating simple aliases/triggers/timers that last only for
the duration of the session. To create durable versions pref Python scripting.


## `/py`

Allows running Python expressions. This is not presently very useful and
requires an overhaul. Be aware you must escape quotes.
