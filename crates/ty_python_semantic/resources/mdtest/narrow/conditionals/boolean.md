# Narrowing for conditionals with boolean expressions

## Narrowing in `and` conditional

```py
class A: ...
class B: ...

def _(x: A | B):
    if isinstance(x, A) and isinstance(x, B):
        reveal_type(x)  # revealed:  A & B
    else:
        reveal_type(x)  # revealed:  (B & ~A) | (A & ~B)
```

## Arms might not add narrowing constraints

```py
class A: ...
class B: ...

def _(flag: bool, x: A | B):
    if isinstance(x, A) and flag:
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: A | B

    if flag and isinstance(x, A):
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: A | B

    reveal_type(x)  # revealed: A | B
```

## Statically known arms

```py
class A: ...
class B: ...

def _(x: A | B):
    if isinstance(x, A) and True:
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: B & ~A

    if True and isinstance(x, A):
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: B & ~A

    if False and isinstance(x, A):
        # TODO: should emit an `unreachable code` diagnostic
        reveal_type(x)  # revealed: Never
    else:
        reveal_type(x)  # revealed: A | B

    if False or isinstance(x, A):
        reveal_type(x)  # revealed: A
    else:
        reveal_type(x)  # revealed: B & ~A

    if True or isinstance(x, A):
        reveal_type(x)  # revealed: A | B
    else:
        # TODO: should emit an `unreachable code` diagnostic
        reveal_type(x)  # revealed: Never

    reveal_type(x)  # revealed: A | B
```

## The type of multiple symbols can be narrowed down

```py
class A: ...
class B: ...

def _(x: A | B, y: A | B):
    if isinstance(x, A) and isinstance(y, B):
        reveal_type(x)  # revealed: A
        reveal_type(y)  # revealed: B
    else:
        # No narrowing: Only-one or both checks might have failed
        reveal_type(x)  # revealed: A | B
        reveal_type(y)  # revealed: A | B

    reveal_type(x)  # revealed: A | B
    reveal_type(y)  # revealed: A | B
```

## Narrowing in `or` conditional

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A) or isinstance(x, B):
        reveal_type(x)  # revealed:  A | B
    else:
        reveal_type(x)  # revealed:  C & ~A & ~B
```

## In `or`, all arms should add constraint in order to narrow

```py
class A: ...
class B: ...
class C: ...

def _(flag: bool, x: A | B | C):
    if isinstance(x, A) or isinstance(x, B) or flag:
        reveal_type(x)  # revealed:  A | B | C
    else:
        reveal_type(x)  # revealed:  C & ~A & ~B
```

## in `or`, all arms should narrow the same set of symbols

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C, y: A | B | C):
    if isinstance(x, A) or isinstance(y, A):
        # The predicate might be satisfied by the right side, so the type of `x` can’t be narrowed down here.
        reveal_type(x)  # revealed:  A | B | C
        # The same for `y`
        reveal_type(y)  # revealed:  A | B | C
    else:
        reveal_type(x)  # revealed:  (B & ~A) | (C & ~A)
        reveal_type(y)  # revealed:  (B & ~A) | (C & ~A)

    if (isinstance(x, A) and isinstance(y, A)) or (isinstance(x, B) and isinstance(y, B)):
        # Here, types of `x` and `y` can be narrowed since all `or` arms constraint them.
        reveal_type(x)  # revealed:  A | B
        reveal_type(y)  # revealed:  A | B
    else:
        reveal_type(x)  # revealed:  A | B | C
        reveal_type(y)  # revealed:  A | B | C
```

## mixing `and` and `not`

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, B) and not isinstance(x, C):
        reveal_type(x)  # revealed:  B & ~C
    else:
        # ~(B & ~C) -> ~B | C -> (A & ~B) | (C & ~B) | C -> (A & ~B) | C
        reveal_type(x)  # revealed: (A & ~B) | C
```

## mixing `or` and `not`

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, B) or not isinstance(x, C):
        reveal_type(x)  # revealed: B | (A & ~C)
    else:
        reveal_type(x)  # revealed: C & ~B
```

## `or` with nested `and`

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A) or (isinstance(x, B) and not isinstance(x, C)):
        reveal_type(x)  # revealed:  A | (B & ~C)
    else:
        # ~(A | (B & ~C)) -> ~A & ~(B & ~C) -> ~A & (~B | C) -> (~A & C) | (~A ~ B)
        reveal_type(x)  # revealed:  C & ~A
```

## `and` with nested `or`

```py
class A: ...
class B: ...
class C: ...

def _(x: A | B | C):
    if isinstance(x, A) and (isinstance(x, B) or not isinstance(x, C)):
        # A & (B | ~C) -> (A & B) | (A & ~C)
        reveal_type(x)  # revealed:  (A & B) | (A & ~C)
    else:
        # ~((A & B) | (A & ~C)) ->
        # ~(A & B) & ~(A & ~C) ->
        # (~A | ~B) & (~A | C) ->
        # [(~A | ~B) & ~A] | [(~A | ~B) & C] ->
        # ~A | (~A & C) | (~B & C) ->
        # ~A | (C & ~B) ->
        # ~A | (C & ~B)  The positive side of ~A is  A | B | C ->
        reveal_type(x)  # revealed:  (B & ~A) | (C & ~A) | (C & ~B)
```

## Boolean expression internal narrowing

```py
def _(x: str | None, y: str | None):
    if x is None and y is not x:
        reveal_type(y)  # revealed: str

    # Neither of the conditions alone is sufficient for narrowing y's type:
    if x is None:
        reveal_type(y)  # revealed: str | None

    if y is not x:
        reveal_type(y)  # revealed: str | None
```

## Assignment expressions

```py
def f() -> bool:
    return True

if x := f():
    reveal_type(x)  # revealed: Literal[True]
else:
    reveal_type(x)  # revealed: Literal[False]
```

## Reassignment in a later operand

An assignment expression in a later operand of `and` or `or` replaces the binding that the earlier
operands narrowed, so their narrowing doesn't apply to the new binding:

```py
class Node:
    parent: "Node | None"

class Name(Node): ...

def _(node: Node | None):
    if isinstance(node, Name) and (node := node.parent) is not None:
        reveal_type(node)  # revealed: Node

def f() -> int | None: ...
def g() -> str | None: ...
def check(value: object) -> bool:
    return True

def _():
    if (x := f()) is None and None is not (x := f()):
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: int | None

def _():
    if (x := f()) is not None and check(x := g()):
        reveal_type(x)  # revealed: str | None

def _():
    if (x := f()) is None or check(x := g()):
        pass
    else:
        reveal_type(x)  # revealed: str | None
```

mypy and pyright infer `Node` in the first example. In the others, mypy infers `int`, even after
`x := g()`. pyright doesn't narrow `x` in the second example, and infers `str | None` in the last
two.

The same applies to operands with statically known truthiness, to attributes of the reassigned name
and to nested boolean expressions. Names that aren't reassigned keep their narrowing:

```py
class C:
    a: int | None

def _():
    if (x := f()) is None and (x := 1):
        reveal_type(x)  # revealed: Literal[1]

def _(c: C):
    if c.a is None and None is not (c := C()).a:
        reveal_type(c.a)  # revealed: int

def _():
    if ((x := f()) is None and check(x)) and None is not (x := f()):
        reveal_type(x)  # revealed: int

def _(y: int | None):
    if y is not None and (x := f()) is not None and check(x := g()):
        reveal_type(y)  # revealed: int
        reveal_type(x)  # revealed: str | None
```
