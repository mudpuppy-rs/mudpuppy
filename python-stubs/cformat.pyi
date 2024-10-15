def cformat(text: str) -> str:
    """
    Return `text` but with tokens of the form `<TOKEN>` replaced with ANSI escape codes.

    See `tokens` for available colour tokens. The special `<reset>` token should be used
    to restore the default terminal colour.

    Example:
    ```python
    from cformat import cformat
    msg = cformat("some <inverted><red>weird red text<reset>, then normal text")
    ````
    """
    ...

def tokens() -> list[str]:
    """
    Return a list of supported replacement tokens.

    Each supported replacement token can be used as
    `<TOKEN>` in a string passed to `cformat`.
    """
    ...
