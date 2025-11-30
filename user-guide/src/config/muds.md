# MUD configuration

Inside your [config file](./index.html), you can define multiple MUD profiles. Each profile can 
have a number of settings that customize the connection to the MUD.

## Example

For example, here is a config file that sets up profiles for three MUDs:

```toml
[[muds]]
name = "DuneMUD (TLS)"
host = "dunemud.net"
port = 6788
tls = "Enabled"

[[muds]]
name = "DunemUD (Telnet)"
host = "dunemud.net"
port = 6789
tls = "Disabled"

[[muds]]
name = "Custom"
host = "dunemud.net"
no_tcp_keepalive = true
hold_prompt = false
echo_input = false
no_line_wrap = true
debug_gmcp = true
splitview_percentage = 50
splitview_margin_horizontal = 0
splitview_margin_vertical = 0
command_separator = ";;"
```


## Fields

Each MUD profile is defined in a `[[muds]]` TOML table in your config file. The following fields
are available for each MUD profile:

| Field                       | Optional | Type   | Default | Examples                                    |
|-----------------------------|----------|--------|---------|---------------------------------------------|
| name                        | No       | String | N/A     | "DuneMUD"                                   |
| host                        | No       | String | N/A     | "dunemud.net", "10.10.10.10"                |
| port                        | No       | int    | N/A     | 4000, 5999                                  |
| tls                         | No       | String | None    | "Enabled","InsecureSkipVerify", "Disabled", |
| echo_input                  | Yes      | bool   | true    |                                             |
| no_line_wrap                | Yes      | bool   | false   |                                             |
| hold_prompt                 | Yes      | bool   | true    |                                             |
| command_separator           | Yes      | String | ";"     | "!", ";;"                                   |
| splitview_percentage        | Yes      | int    | 70      |                                             |
| splitview_margin_horizontal | Yes      | int    | 6       |                                             | 
| splitview_margin_vertical   | Yes      | int    | 0       |                                             |
| no_tcp_keepalive            | Yes      | bool   | false   |                                             |
| debug_gmcp                  | Yes      | bool   | false   |                                             |


### Name

The name of the MUD profile. This is used to identify the MUD in the MUD list screen and in the 
`--connect` command line option. It is also the title for your session tab when connected.

You can write triggers and aliases that only apply for connections to a MUD with a specific name
matching this config field.

### Host

The hostname of the MUD server. This can be a domain name, or an IP address (both IPv4 and IPv6 are 
supported). 

When connecting to a domain name Mudpuppy uses the ["happy eyeballs"] algorithm, which means
it will try both IPv4 and IPv6 connections in parallel and use whichever succeeds first.

["happy eyeballs"]: https://en.wikipedia.org/wiki/Happy_Eyeballs

### Port

The port number of the MUD server. This is the port that the MUD server is listening on for
connections. Make sure the [TLS](#tls) setting matches the port number you use. Some MUD servers
only speak TLS on a specific port and assume telnet will be used for other ports.

### TLS

The [transport layer security] setting for the connection. It's **recommended** to use TLS
to connect to a MUD when possible to avoid sending your username/password and all other data in
plaintext on the network.

The available option values are:

* **"Enabled"**: TLS will be used for the connection and the MUD server's certificate will be
  verified. If the certificate is not valid, the connection will be refused. This is the
  recommended setting when using TLS.
* **"InsecureSkipVerify"**: TLS will be used for the connection, but the MUD server's certificate
  will not be verified. This should only be used for testing purposes since it is insecure.
* **"Disabled"**: The connection will be made over plain text (telnet) without using TLS. This is
  not recommended unless you have no other choice.

[transport layer security]: https://en.wikipedia.org/wiki/Transport_Layer_Security

## echo_input

When set to `true` (the default) Mudpuppy will display your sent input in the output buffer.
This is useful for seeing what you've sent to the game. If the input you sent matched an
alias, both the original input you sent, and the expanded alias will be shown.

When set to `false` Mudpuppy will not display your sent input in the output buffer. This
can be useful if you prefer not to clutter your output buffer with your own input history.

### no_line_wrap

When set to `false` (the default) Mudpuppy will wrap long lines of text in the output buffer
so they aren't truncated. This is useful for reading long lines of text that don't fit in the
window.

When set to `true` Mudpuppy will not wrap long lines of text in the output buffer. This can be
helpful if you prefer to preserve the layout of the text as sent by the MUD. It may mean
that some parts of the text are not visible without resizing your terminal window to be wide
enough to accommodate the full text.

### hold_prompt

When set to `true` (the default) Mudpuppy will automatically "hold" the last received
prompt line at the bottom of the screen. This is helpful if you want to see your prompt 
at all times.

See [prompt detection](../scripting/prompts.md) for more information on how Mudpuppy
determines what is/isn't a prompt.

You may wish to set this to `false` if:

* You prefer to have your prompt printed as a normal line in the output buffer.
* Mudpuppy fails to detect the prompt correctly.

### command_separator

The command separator is a string that Mudpuppy uses to split input into multiple commands.
By default, this is `;`. This means that if you type `say hello;wave` and hit enter, Mudpuppy
will send `say hello` and `wave` as separate commands to the MUD.

See [command splitting](../input.md#command-splitting) for more information.

### splitview_percentage

The percentage of the screen that the scrollback history window should take up. This is a 
number between 0 and 100. The default is 70%.

### splitview_margin_horizontal

The number of columns of space to use as margin on the left/right side of the scrollback
history window. The default is 6. If you set this to 0 the scrollback history window
will not have any margin and will cover the output buffer completely on the left/right.

### splitview_margin_vertical

The number of rows of space to use as margin on the top/bottom of the scrollback history
window. The default is 0. If you set this to 10 the scrollback history window will show
10 rows of the output buffer above/below the scrollback window.

### no_tcp_keepalive

When set to `false` (the default) Mudpuppy will send TCP keepalive packets to the MUD server
periodically. This is useful because Telnet has no built-in keepalive mechanism.

By adding `no_tcp_keepalive = true` to a MUD configuration Mudpuppy will not send keepalives.
You may find this makes your connections drop after a period of inactivity.

### debug_gmcp

When set to `true` Mudpuppy will print received GMCP messages to the output buffer as
[debug output]. This can be useful for debugging GMCP issues with a MUD, but is also very
verbose!

[debug output]: ../scripting/output.md#debug-output
