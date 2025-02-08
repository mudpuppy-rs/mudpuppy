# IDE Setup for Script Editing

Once your scripts reach a certain complexity level it's helpful to have a Python
integrated development environment (IDE) set up that understand the Mudpuppy
APIs.

Since the Mudpuppy APIs are only exposed from inside of Mudpuppy this requires
a little bit of special configuration to tell your IDE where to find "[stub
files]" describing the API. How you do this depends on the specific IDE or
tool. This page describes doing it with [VSCode].

In both cases you'll need the `.pyi` stub files from the [Mudpuppy GitHub
repo]. You can find them under the [python-stubs] directory.

[stub files]: <https://mypy.readthedocs.io/en/stable/stubs.html>
[VSCode]: https://code.visualstudio.com/
[Mudpuppy GitHub repo]: https://github.com/mudpuppy-rs/mudpuppy
[python-stubs]: https://github.com/mudpuppy-rs/mudpuppy/tree/main/python-stubs


## Visual Studio Code

### Python extension

You'll want to use the **Python** VSCode extension for editing Mudpuppy python
scripts. It comes with the **Pylance** extension that will be used for
type checking.

After installing the extension:

1. Click on `File` -> `Options` -> `Settings`
2. Search for the `python.languageServer` option; select '**Pylance**'.
3. Restart VS Code.

See the [VSCode python docs] for more information.

[VSCode python docs]: https://code.visualstudio.com/docs/languages/python

### Setup stubs

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

If you want to use a folder name other than `typings` you'll need to customize
the `python.analysis.stubPath` option. See the [VSCode settings reference] for
more information.

[VSCode settings reference]: https://code.visualstudio.com/docs/python/settings-reference

### Further customization

You may wish to configure the integration further, e.g. to disable missing
module source warnings.

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
