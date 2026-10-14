## What it does

Detects method overrides that rename a positional-or-keyword parameter of the overridden method, or
make it positional-only, but are otherwise compatible with the overridden method.

This rule is disabled by default. Enable it to also require overriding methods to accept the
arguments of the overridden method by keyword. Other incompatible overrides are reported by
`invalid-method-override`. That includes an override that accepts the original name of such a
parameter in another way, for example as a keyword-only parameter or through `**kwargs`, since a
call that passes the argument by keyword would bind it to a different parameter.

## Why is this bad?

An argument for a positional-or-keyword parameter can be passed by keyword, using the name of the
parameter. A call that passes such an argument by keyword to a method of a superclass fails at
runtime if the object is an instance of a subclass whose override renamed the parameter or made it
positional-only. This violates the [Liskov Substitution Principle][liskov-substitution-principle].

Such an override still accepts every call that passes these arguments positionally, and many methods
are only ever called that way.

## Example

```python
class Super:
    def method(self, x: int) -> None: ...


class Sub(Super):
    def method(self, y: int) -> None: ...  # error: [invalid-parameter-name-override]


class Sub2(Super):
    def method(self, x: int, /) -> None: ...  # error: [invalid-parameter-name-override]


def call_with_keyword(obj: Super) -> None:
    obj.method(x=42)


# TypeError at runtime: Sub.method() got an unexpected keyword argument 'x'
call_with_keyword(Sub())
```

Use the parameter names of the overridden method in the override. If callers are not meant to pass
an argument by keyword, make the parameter positional-only in the overridden method as well.

[liskov-substitution-principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle
