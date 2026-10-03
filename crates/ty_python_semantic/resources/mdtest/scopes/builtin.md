# Builtin scope

## Conditional local override of builtin

If a builtin name is conditionally shadowed by a local variable, the function's binding scope
terminates name resolution. The name can be unbound, but it cannot refer to the builtin:

```py
def _(flag: bool) -> None:
    if flag:
        abs = 1
        chr: int = 1

    # error: [possibly-unresolved-reference]
    reveal_type(abs)  # revealed: Literal[1]
    # error: [possibly-unresolved-reference]
    reveal_type(chr)  # revealed: Literal[1]
```

## Conditionally global override of builtin

If a builtin name is conditionally shadowed by a global variable, a name lookup should union the
builtin type with the conditionally-defined type:

```py
def flag() -> bool:
    return True

if flag():
    abs = 1
    chr: int = 1

def _():
    # TODO: Should ideally be `Literal[1] | (def abs(x: SupportsAbs[_T], /) -> _T)`
    reveal_type(abs)  # revealed: Literal[1]
    # TODO: Should ideally be `int | (def chr(i: SupportsIndex, /) -> str)`
    reveal_type(chr)  # revealed: int
```

## Global override of builtin in unreachable code

A module that binds a builtin name only in unreachable code, for example in a branch for an older
Python version, never binds that name at runtime. Function bodies and deferred annotations therefore
see the builtin, just like code at module level does.

### Python 2 compatibility code

```toml
[environment]
python-version = "3.14"
```

```py
import sys

PY2 = sys.version_info[0] == 2

if PY2:
    str = unicode
    range = xrange

reveal_type(range)  # revealed: <class 'range'>

# TODO: no error
# error: 10 [invalid-type-form]
# error: 18 [invalid-type-form]
def f(x: str) -> str:
    # TODO: should be `str`
    reveal_type(x)  # revealed: Unknown
    # TODO: should be `<class 'range'>`
    reveal_type(range)  # revealed: Never
    return x

def outer():
    # TODO: no error
    # error: 18 [invalid-type-form]
    # error: 26 [invalid-type-form]
    def inner(x: str) -> str:
        # TODO: should be `str`
        reveal_type(x)  # revealed: Unknown
        # TODO: should be `<class 'range'>`
        reveal_type(range)  # revealed: Never
        return x

class C:
    # TODO: no error
    # error: [invalid-type-form]
    attribute: str

# TODO: should be `str`
reveal_type(C().attribute)  # revealed: Unknown

# TODO: no error
# error: [invalid-type-form]
type Strings = list[str]

def g(strings: Strings):
    # TODO: should be `list[str]`
    reveal_type(strings)  # revealed: list[Unknown]
```

### Annotations deferred with `from __future__ import annotations`

```py
from __future__ import annotations

import sys

if sys.version_info < (3, 0):
    str = unicode

# TODO: no error
# error: 10 [invalid-type-form]
# error: 19 [invalid-type-form]
# error: 28 [invalid-type-form]
def f(x: str, y: "str") -> str:
    # TODO: should be `str`
    reveal_type(x)  # revealed: Unknown
    # TODO: should be `str`
    reveal_type(y)  # revealed: Unknown
    return x
```

### `exceptiongroup` backport

The `exceptiongroup` package backports `ExceptionGroup` and `BaseExceptionGroup`, which are builtins
since Python 3.11. mypy and pyright also resolve these names to the builtin classes here:

```toml
[environment]
python-version = "3.11"
```

```py
from __future__ import annotations

import sys

if sys.version_info < (3, 11):
    from exceptiongroup import BaseExceptionGroup, ExceptionGroup

def handle(group: ExceptionGroup[ValueError]) -> None:
    # TODO: should be `ExceptionGroup[ValueError]`
    reveal_type(group)  # revealed: Unknown

def check(exc: BaseException) -> None:
    if isinstance(exc, BaseExceptionGroup):
        # TODO: should be `BaseExceptionGroup[Unknown]`
        reveal_type(exc)  # revealed: BaseException

def get_class() -> None:
    # TODO: should be `<class 'ExceptionGroup'>`
    reveal_type(ExceptionGroup)  # revealed: Never
```

### `exceptiongroup` backport on Python 3.10

On Python 3.10, the import is reachable, so the same lookups find the backported classes:

```toml
[environment]
python-version = "3.10"
```

`exceptiongroup.py`:

```py
class BaseExceptionGroup(BaseException): ...
class ExceptionGroup(BaseExceptionGroup, Exception): ...
```

```py
import sys

if sys.version_info < (3, 11):
    from exceptiongroup import BaseExceptionGroup, ExceptionGroup

def check(exc: BaseException) -> None:
    if isinstance(exc, BaseExceptionGroup):
        reveal_type(exc)  # revealed: BaseExceptionGroup

def get_class() -> None:
    reveal_type(ExceptionGroup)  # revealed: <class 'ExceptionGroup'>
```

### Names that are not builtins

If a name that is only bound in unreachable code is not a builtin, nothing binds it at runtime. In a
function body, we infer `Never` for such a name without reporting an unresolved reference, as for
other symbols that are only defined in unreachable code. mypy and pyright report the name as
undefined in the function body as well:

```py
import sys

if sys.platform == "win32":
    import winreg

def f():
    reveal_type(winreg)  # revealed: Never

# error: [unresolved-reference]
winreg
```
