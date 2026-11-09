# A sample that exercises the Python grammar's highlight captures.
from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import Generic, TypeVar

T = TypeVar("T")
LIMIT = 0x10
RATIO = 1.5e-3


@dataclass(frozen=True)
class Point(Generic[T]):
    """A point with two coordinates."""

    x: T
    y: T
    tags: list[str] = field(default_factory=list)

    def length(self) -> float:
        return (self.x**2 + self.y**2) ** 0.5

    @property
    def label(self) -> str:
        return f"({self.x!r}, {self.y:.2f}) \t{len(self.tags)}"


class Store:
    def __init__(self, *items: int, **named: str) -> None:
        self._items = {i: str(i) for i in items if i is not None}
        self.named = named

    def __len__(self) -> int:
        return len(self._items)

    async def load(self, keys: list[int]) -> int:
        count = 0
        for key in keys:
            await asyncio.sleep(0)
            if key in self._items and not key < 0:
                count += 1
            elif key == LIMIT:
                break
            else:
                continue
        return count


def parse(text: str | None = None) -> list[str]:
    try:
        lines = [line.strip() for line in (text or "").splitlines()]
    except (ValueError, TypeError) as error:
        raise RuntimeError("bad text") from error
    finally:
        pass
    match lines:
        case [first, *rest]:
            return [first, *rest]
        case _:
            return []


def main() -> None:
    raw = r"C:\path\with 'quotes'"
    data = b"\x00\xff"
    total = sum(x * 2 for x in range(LIMIT) if x % 2 == 0)
    square = lambda n: n * n
    with open(__file__, encoding="utf-8") as handle:
        print(raw, data, total, square(3), handle.name, True, False, None, ...)
    global RATIO
    assert RATIO > 0, "positive"
    del total


if __name__ == "__main__":
    main()
