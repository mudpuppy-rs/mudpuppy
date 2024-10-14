"""
Output pdoc[0] API documentation for the mudpuppy Python modules.

Unlike other files in this directory, this .py built-in module
is only loaded for --features=__pdoc builds and is meant to be
used by CI, not end-users.

[0]: https://pdoc.dev/
"""

from pathlib import Path
from pdoc import doc, render, extract
import mudpuppy_core

render.configure(
    docformat="markdown",
    template_directory=Path("pdoc-templates"),
    show_source=False,
)

output_directory = Path("web/api-docs")
to_document = ["mudpuppy", "cformat", "layout", "commands"]

all_modules = {}

# Customize mudpuppy_core for better documentation. Since this isn't a plain .py
# We need to twiddle a pdoc.doc.Module for it directly.
core_module = doc.Module(mudpuppy_core)
core_module.obj.__all__ = ["Event"]
all_modules["mudpuppy_core"] = core_module

# For the other built-in modules we can use the .py files directly.
for module_name in extract.walk_specs(to_document):
    all_modules[module_name] = doc.Module.from_name(module_name)

# The code that follows is largely a re-impl of pdoc.pdoc().
for module in all_modules.values():
    out = render.html_module(module, all_modules)
    outfile = output_directory / f"{module.fullname.replace('.', '/')}.html"
    outfile.parent.mkdir(parents=True, exist_ok=True)
    outfile.write_bytes(out.encode())

index = render.html_index(all_modules)
if index:
    (output_directory / "index.html").write_bytes(index.encode())

search = render.search_index(all_modules)
if search:
    (output_directory / "search.js").write_bytes(search.encode())
