# Errors

s = set()


def _():
    for x in [1, 2, 3]:
        s.add(x)


def _():
    for x in {1, 2, 3}:
        s.add(x)


def _():
    for x in (1, 2, 3):
        s.add(x)


def _():
    for x in (1, 2, 3):
        s.discard(x)


def _():
    for x in (1, 2, 3):
        s.add(x + 1)


def _():
    for x, y in ((1, 2), (3, 4)):
        s.add((x, y))


num = 123


def _():
    for x in (1, 2, 3):
        s.add(num)


def _():
    for x in (1, 2, 3):
        s.add((num, x))


def _():
    for x in (1, 2, 3):
        s.add(x + num)


# https://github.com/astral-sh/ruff/issues/15936
def _():
    for x in 1, 2, 3:
        s.add(x)


def _():
    for x in 1, 2, 3:
        s.add(f"{x}")


def _():
    for x in (
        1,  # Comment
        2, 3
    ):
        s.add(f"{x}")


# False negative

class C:
    s: set[int]


def _():
    c = C()
    for x in (1, 2, 3):
        c.s.add(x)


# Ok

s.update(x for x in (1, 2, 3))


def _():
    for x in (1, 2, 3):
        s.add(x)
    else:
        pass


async def f(y):
    async for x in y:
        s.add(x)


def g():
    for x in (set(),):
        x.add(x)


# Test cases for lambda and ternary expressions - https://github.com/astral-sh/ruff/issues/18590

s = set()


def _():
    for x in lambda: 0:
        s.discard(-x)


def _():
    for x in (1,) if True else (2,):
        s.add(-x)


# don't add extra parens
def _():
    for x in (lambda: 0):
        s.discard(-x)


def _():
    for x in ((1,) if True else (2,)):
        s.add(-x)


# don't add parens directly in function call
def _():
    for x in lambda: 0:
        s.discard(x)


def _():
    for x in (1,) if True else (2,):
        s.add(x)


# https://github.com/astral-sh/ruff/issues/21098
def _():
    for x in ("abc", "def"):
        s.add(c for c in x)


# don't add extra parens for already parenthesized generators
def _():
    for x in ("abc", "def"):
        s.add((c for c in x))


# https://github.com/astral-sh/ruff/issues/18575
# Ok: the loop variable is used outside the loop
def _():
    for x in (2, 3):
        s.add(x)
    print(x)


def _():
    for x, y in ((1, 2), (3, 4)):
        s.discard((x, y))
    print(y)


def _():
    for x in (2, 3):
        s.add(x)

    def f():
        return x


# Ok: the loop variable rebinds a global or an earlier binding
def _():
    global CURRENT
    for CURRENT in (2, 3):
        s.add(CURRENT)


def _():
    x = 1
    for x in (2, 3):
        s.add(x)
