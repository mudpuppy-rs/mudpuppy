# Command line

Mudpuppy offers a number of command line flags to customize its behavior. 

## Help

You can run `mudpuppy --help` to see the available options:

```
Usage: mudpuppy [OPTIONS]

Options:
  -f, --frame-rate <FLOAT>  Frame rate, i.e. number of frames per second [default: 60]
  -c, --connect <MUD_NAME>  MUD name to auto-connect to at startup. Can be specified multiple times
  -l, --log-level <LEVEL>   Log level filter. Default is INFO [default: INFO]
  -h, --help                Print help
  -V, --version             Print version
```

## Connect

By default `mudpuppy` opens to a MUD list screen where you can select which MUD to connect to based on the ones
listed in your [Config]. However, if you know which MUD(s) you want to connect to at startup, you can use the
`--connect` option to specify them. This option can be used multiple times to specify multiple MUDs. Mudpuppy 
will open new tabs for each of the `--conect` arguments and immediately connect. The `<MUD_NAME>` argument must
match the `name` field of a MUD in your [MUD Config].

[MUD Config]: ./config/muds.md

## Log Level

Controls the verbosity of the log output. The `--log-level` option lets you specify the minimum log level to display.
See [Logging] for more information on the available log levels.

[Logging]: ./logging.md

## Frame Rate

The `--frame-rate` option lets you customize the client frame rate.

Mudpuppy uses an [immediate mode] (IM) terminal user interface (TUI). This means that each frame, the portions of the
interface that have changed are redrawn. The frame rate argument specifies how many frames per second Mudpuppy should
aim for. The default is 60 frames per second, giving a nice smooth interface.

You may find (especially since Mudpuppy is an unoptimized prototype!) that drawing at this frame rate uses excessive
CPU. First confirm you're running a `--release` build (debug builds are **significantly** slower). After that, try
experimenting with lowering the frame rate. This will reduce the CPU usage, but may increase interface lag (e.g.
when responding to your keystrokes).

[immediate mode]: https://en.wikipedia.org/wiki/Immediate_mode_(computer_graphics)