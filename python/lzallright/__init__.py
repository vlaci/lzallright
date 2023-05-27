"""lzalright LZO compression library.

A Python 3.8+ binding for [LZ👌](https://github.com/jackoalan/lzokay) library which is

> A minimal, C++14 implementation of the
> [LZO compression format](http://www.oberhumer.com/opensource/lzo/).
"""

from lzallright._lzallright import EResult, InputNotConsumed, LZOCompressor, LZOError

__all__ = [
    "EResult",
    "InputNotConsumed",
    "LZOCompressor",
    "LZOError",
]
