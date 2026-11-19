<!-- A sample that exercises the Markdown grammar's highlight captures and injections. -->

# A heading with `code` and *emphasis*

Some prose with **strong** text, _underscored emphasis_, a [link](https://example.com)
and an ![image](a.png "title"), an autolink <https://example.org> and a \*escape\*.

## Lists

- An item with `inline code`
- Another item that spans
  a second line with *stars*
  1. A numbered item
  2. Another one

> A block quote with **strong** words
> across two lines.

Setext heading
--------------

```rust
fn main() {
    let a = vec![1, 2, 3];
    println!("{}", a.len());
}
```

```py
def area(r: float) -> float:
    return 3.14 * r * r
```

```json
{"a": [1, true, null]}
```

```unknown-language
plain text in a fence
```

    an indented code block

<div class="note">
  <p>An HTML block.</p>
</div>

---

[ref]: https://example.com/reference "A title"

The end, with a ~~strike~~ and a math $x$ marker.
