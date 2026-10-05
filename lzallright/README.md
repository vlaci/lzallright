The crate is started as a port of the C++ [lzokay] library.

[lzokay]: https://github.com/jackoalan/lzokay

> LZO compresses a block of data into matches (a sliding
> dictionary) and runs of non-matching literals to produce good
> results on highly redundant data

— [Wikipedia](https://en.wikipedia.org/wiki/Lempel%E2%80%93Ziv%E2%80%93Oberhumer)

There is no publicly available documentation of the LZO format. A
good introduction is available in the [Linux Kernel documentation]

[Linux Kernel Documentation]: https://docs.kernel.org/staging/lzo.html

The crate is `no_std`. Decompression works without any allocation.
For compression, a [`Dict`] instance is needed.


# Examples

```rust
use lzallright::{compress, decompress, worst_case_len, Dict};

let input = b"hello world";

// You should reuse the dictionary across calls.
let mut dict = Dict::new();
let mut compressed = vec![0u8; worst_case_len(input.len())];
let n = compress(input, &mut compressed, &mut dict).unwrap();
compressed.truncate(n);

let mut output = vec![0u8; input.len()];
let m = decompress(&compressed, &mut output).unwrap();
assert_eq!(&output[..m], input);
```
