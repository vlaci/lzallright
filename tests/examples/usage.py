from lzallright import EResult, InputNotConsumed, LZOCompressor

data = b"Hello World! " * 100

compressor = LZOCompressor()  # (1)!
compressed = compressor.compress(data)  # (2)!

decompressed = LZOCompressor.decompress(compressed)  # (3)!
assert decompressed == data

decompressed = LZOCompressor.decompress(compressed, output_size_hint=len(data))  # (4)!
assert decompressed == data

try:
    LZOCompressor.decompress(compressed + b"trailing garbage")
except InputNotConsumed as e:  # (5)!
    reason, decompressed = e.args
    assert reason == EResult.InputNotConsumed
    assert decompressed == data
