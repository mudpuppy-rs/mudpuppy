# IDE Setup for Script Editing

Once your scripts reach a certain complexity level it's helpful to have a Python
integrated development environment (IDE) set up that understand the Mudpuppy
APIs.

Since the Mudpuppy APIs are only exposed from inside of Mudpuppy this requires
a little bit of special configuration to tell your IDE where to find "[stub
files]" describing the API. How you do this depends on the specific IDE or
tool. This page describes doing it with [VSCode], [PyCharm] and [Pyright].

In both cases you'll need the `.pyi` stub files from the [Mudpuppy GitHub
repo]. You can find them under the [python-stubs] directory.

[stub files]: <https://mypy.readthedocs.io/en/stable/stubs.html>
[VSCode]: https://code.visualstudio.com/
[PyCharm]: https://www.jetbrains.com/pycharm/
[Pyright]: https://github.com/microsoft/pyright
[Mudpuppy GitHub repo]: https://github.com/mudpuppy-rs/mudpuppy
[python-stubs]: https://github.com/mudpuppy-rs/mudpuppy/tree/main/python-stubs

## Setup Stubs

First, make sure you've taken note of where you cloned Mudpuppy, and the
location of the `python-stubs` directory inside.

On Linux, MacOS or in WSL, you can **symlink** the Mudpuppy stubs into your
project directory. That way they're always up to date with your clone of
Mudpuppy:

```
ln -s /path/to/mudpuppy/python-stubs ./typings
```

If you're on Windows, you can copy the files instead, but remember to update
them as Mudpuppy changes!

```
xcopy /E /I \path\to\mudpuppy\python-stubs .\typings
```

## Visual Studio Code

### Python Extension

You'll want to use the **Python** VSCode extension for editing Mudpuppy python
scripts. It comes with the **Pylance** extension that will be used for
type checking.

After installing the extension:

1. Click on `File` -> `Options` -> `Settings`
2. Search for the `python.languageServer` option; select '**Pylance**'.
3. Restart VS Code.

### Type Checking

If you've copied the stub files to a directory named `typings` in the root of
your project, no further configuration is needed. If you want to use a folder
name other than `typings` you'll need to customize the
`python.analysis.stubPath` option. See the [VSCode settings reference] and
[VSCode python docs] for more information.

[VSCode python docs]: https://code.visualstudio.com/docs/languages/python

[VSCode settings reference]: https://code.visualstudio.com/docs/python/settings-reference

### Missing Module Source Warnings

Since the Mudpuppy stubs are just that, stubs, they don't have associated source
code. To stop VSCode from warning you about that we need to customize it
further:

1. Click on `File` -> `Options` -> `Settings`
2. Search for the `python.analysis.diagnosticSeverityOverrides` option. We want
   to suppress `'reportMissingModuleSource'`.
3. Restart VS Code.

This is optional, but will clear up any warnings you might see about a missing
source module.

Here's an example of this section of VS Code's json settings after making the
change:

```json
"python.analysis.diagnosticSeverityOverrides": {
  "reportMissingModuleSource": "none"
}
```

## PyCharm

After copying the stub files into the root of your project you can:

1. Right-click the directory in the project source tree view.
2. Select "Mark directory as"
3. Select "Source root"

That's it! You're all set.

For more information see the [PyCharm stubs documentation].

[PyCharm stubs documentation]: https://www.jetbrains.com/help/pycharm/stubs.html

## Pyright

You can also configure a static type checking tool like [Pyright] to use the
Mudpuppy stubs. This can be helpful for command-line type checking, or CI
integrations.

1. Install `pyright` with `pip install pyright`
2. Create (or update) a `pyproject.toml` file at the root of your project with
   contents:
```toml
[tool.pyright]
stubPath = "typings" # or whatever directory name you used for the stubs
reportMissingModuleSource = false
```

See the [Pyright user manual] for more information.

[Pyright user manual]: https://microsoft.github.io/pyright/
