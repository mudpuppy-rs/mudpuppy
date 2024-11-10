# Input

## Command splitting

Often it's useful to be able to enter several commands all in one go. For this
situation Mudpuppy supports input that's split into multiple commands based on
a special command splitting delimiter. 

By default this delimiter is `;;` but it can be adjusted by setting the
`command_separator` field of a MUD config in your [Config] file.

[Config]: config.md

### Example

For example, if you typed the following input and hit enter:

```
say hello;;wave;;/status -v
```

Then Mudpuppy would send these commands to the MUD:

1. `say hello`
2. `wave`

and then it would run the `/status -v` [command].

If you've defined an [alias] for the pattern `^wave$` (for example) it would be
evaluated just like if you typed `wave` without using the `;;` splitter.

Remember if you've changed the `command_separator` you'll have to adjust the
example above.

[command]: commands.md
[alias]: scripting/aliases.md
