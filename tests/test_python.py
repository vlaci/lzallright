import array
import mmap
import os
from concurrent.futures import ThreadPoolExecutor

import pytest

import lzallright


@pytest.fixture
def lorem(request):
    return (request.session.path / "benches/lorem.txt").read_bytes()


@pytest.mark.parametrize("size_hint", [None, 100, 128 * 2**10])
def test_roundtrip(lorem, size_hint):
    c = lzallright.LZOCompressor()
    comp = c.compress(lorem)

    assert (
        lzallright.LZOCompressor.decompress(comp, output_size_hint=size_hint) == lorem
    )


def test_decompress_partial(lorem):
    c = lzallright.LZOCompressor()
    comp = c.compress(lorem)

    with pytest.raises(lzallright.InputNotConsumed) as exc:
        lzallright.LZOCompressor.decompress(comp + b"foobar")

    assert exc.value.args == (lzallright.EResult.InputNotConsumed, lorem)
    assert issubclass(exc.type, lzallright.LZOError)


def test_decompress_error(lorem):
    with pytest.raises(lzallright.LZOError) as exc:
        lzallright.LZOCompressor.decompress(lorem)

    assert exc.value.args == (lzallright.EResult.LookbehindOverrun,)


def roundtrip(data, **kwargs):
    comp = lzallright.LZOCompressor().compress(data)
    return lzallright.LZOCompressor.decompress(comp, **kwargs)


@pytest.mark.parametrize(
    "data",
    [
        bytearray(b"hello world" * 10),
        memoryview(b"hello world" * 10),
        array.array("i", range(100)),
    ],
    ids=type,
)
def test_buffer_types(data):
    assert roundtrip(data) == memoryview(data).tobytes()


def test_mmap():
    with mmap.mmap(-1, 4096) as m:
        m.write(b"x" * 4096)
        assert roundtrip(m) == b"x" * 4096


@pytest.mark.xfail(strict=True, reason="buffers are not yet validated")
def test_non_contiguous_buffer_is_rejected():
    with pytest.raises(BufferError):
        lzallright.LZOCompressor().compress(memoryview(b"abcdefgh" * 10)[::2])


def test_truncated_input():
    with pytest.raises(lzallright.LZOError) as exc:
        lzallright.LZOCompressor.decompress(b"\x11\x00")

    assert exc.value.args == (lzallright.EResult.InputOverrun,)
