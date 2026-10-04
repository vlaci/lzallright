"""lzalright LZO compression library.

A Python 3.11+ implementation of the
[LZO compression format](http://www.oberhumer.com/opensource/lzo/) in safe Rust,
ported from the C++ [LZ👌](https://github.com/jackoalan/lzokay) library.
"""

from lzallright._lzallright import EResult, InputNotConsumed, LZOCompressor, LZOError

__all__ = [
    "EResult",
    "InputNotConsumed",
    "LZOCompressor",
    "LZOError",
]
