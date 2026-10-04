# Usage

The following gist is to help you getting started using this library

```python
--8<-- "tests/examples/usage.py"
```

1.  Create a compressor. Reuse the instance to reuse its working memory across calls
2.  Compress any object implementing the buffer protocol (`bytes`, `bytearray`, `memoryview`, ...)
3.  Decompression is a static method, no instance needed
4.  Pass the expected size, if known, to avoid reallocating the output buffer
5.  Trailing data after a valid stream raises [`InputNotConsumed`][lzallright._lzallright.InputNotConsumed], where you can grab the successfully decompressed data
