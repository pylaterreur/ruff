# `__str__` and `__repr__`

```py
from typing_extensions import Literal, LiteralString
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

def _(
    a: Literal[1],
    b: Literal[True],
    c: Literal[False],
    d: Literal["ab'cd"],
    e: Literal[Answer.YES],
    f: LiteralString,
    g: int,
):
    reveal_type(str(a))  # revealed: str
    reveal_type(str(b))  # revealed: str
    reveal_type(str(c))  # revealed: str
    reveal_type(str(d))  # revealed: str
    reveal_type(str(e))  # revealed: str
    reveal_type(str(f))  # revealed: str
    reveal_type(str(g))  # revealed: str

    reveal_type(repr(a))  # revealed: Literal["1"]
    reveal_type(repr(b))  # revealed: Literal["True"]
    reveal_type(repr(c))  # revealed: Literal["False"]
    # TODO: should be `Literal["\"ab'cd\""]`
    reveal_type(repr(d))  # revealed: Literal["'ab\\'cd'"]
    # TODO: this could be `<Answer.YES: 1>`
    reveal_type(repr(e))  # revealed: str
    reveal_type(repr(f))  # revealed: LiteralString
    reveal_type(repr(g))  # revealed: str
```

`repr()` of a string literal follows CPython: it uses single quotes unless the string contains
single quotes but no double quotes, and it escapes backslashes, the quote character, and
non-printable characters. mypy and pyright infer `str` for these calls.

```py
reveal_type(repr("abc"))  # revealed: Literal["'abc'"]
# TODO: should be `Literal["\"it's\""]`
reveal_type(repr("it's"))  # revealed: Literal["'it\\'s'"]
# TODO: should be `Literal["'say \"hi\"'"]`
reveal_type(repr('say "hi"'))  # revealed: Literal["'say \\\"hi\\\"'"]
# TODO: should be `Literal["'it\\'s \"hi\"'"]`
reveal_type(repr('it\'s "hi"'))  # revealed: Literal["'it\\'s \\\"hi\\\"'"]
reveal_type(repr("a\\b"))  # revealed: Literal["'a\\\\b'"]
# TODO: should be `Literal["'\\t\\n\\r\\x00\\x7f'"]`
reveal_type(repr("\t\n\r\0\x7f"))  # revealed: Literal["'\\t\\n\\r\\u{0}\\u{7f}'"]
# TODO: should be `Literal["'é\\x85\\xa0\\xad'"]`
reveal_type(repr("é\x85\xa0\xad"))  # revealed: Literal["'\\u{e9}\\u{85}\\u{a0}\\u{ad}'"]
```

Which characters are printable depends on the Unicode database of the Python version that runs the
code. Above U+00FF (outside Latin-1), this can differ between Python versions, so we infer `str`:

```py
# TODO: should be `str`
reveal_type(repr("⦂"))  # revealed: Literal["'\\u{2982}'"]
```
