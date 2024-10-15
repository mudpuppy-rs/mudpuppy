"""
Output pdoc[0] API documentation for the mudpuppy Python modules.

[0]: https://pdoc.dev/
"""

import os
from pathlib import Path
from typing import List, Set
from pdoc import doc, render, extract


def rename_pyi_to_py(directory: Path) -> [List[str], List[Path]]:
    """
    pdoc can't work with stub files, but the stubs can stand in as .py
    files in a pinch as long as we rename them back after.

    :param directory: location of .pyi stub files (no subdirectories)
    :return: the list of modules renamed, and the list of renamed files.
    """
    renamed_files: List[Path] = []
    base_names: List[str] = []

    for file in directory.iterdir():
        if file.suffix == ".pyi" and file.is_file():
            py_file = file.with_suffix(".py")
            base_names.append(file.stem)
            os.rename(file, py_file)
            renamed_files.append(py_file)

    return base_names, renamed_files


def restore_files(renamed_files: List[Path]) -> None:
    """
    Restores the .pyi files from the .py files we renamed to generate docs.

    :param renamed_files: list of files that were renamed by `rename_pyi_to_py`
    """
    for file in renamed_files:
        original_file = file.with_suffix(".pyi")
        os.rename(file, original_file)


def render_docs(modules: Set[str]) -> None:
    """
    Uses the list of renamed stub files to generate API documentation into the web dir.

    :param modules: the list of stub modules to write docs for.
    """
    render.configure(
        docformat="markdown",
        template_directory=Path("../pdoc-templates"),
        show_source=False,
    )

    output_directory = Path("../web/api-docs")

    all_modules = {}
    for module_name in extract.walk_specs(modules):
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


def main() -> None:
    """
    Process the stub files in the same directory as the script. Generating API documentation for
    each.
    """
    script_directory = Path(__file__).parent
    base_names, renamed_files = rename_pyi_to_py(script_directory)
    if len(base_names) == 0:
        print("No stub .pyi files found.")
        return
    try:
        render_docs({"mudpuppy_core", "mudpuppy", *base_names})
    except Exception as e:
        print(e)
    finally:
        restore_files(renamed_files)


if __name__ == "__main__":
    main()
