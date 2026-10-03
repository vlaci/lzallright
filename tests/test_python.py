import array
import mmap
import os
import sys
import threading
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


def test_output_buffer_grows_from_zero_hint(lorem):
    assert roundtrip(lorem, output_size_hint=0) == lorem


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


def test_non_contiguous_buffer_is_rejected():
    with pytest.raises(BufferError):
        lzallright.LZOCompressor().compress(memoryview(b"abcdefgh" * 10)[::2])


@pytest.mark.skipif(
    not getattr(sys, "_is_gil_enabled", lambda: True)(),
    reason="copying races with writers without the GIL",
)
def test_readonly_view_of_mutable_buffer_is_snapshotted():
    # A read-only view does not make the exporter immutable: the bytearray is
    # rewritten while the GIL is released, so a borrowed input would mix both.
    a, b = os.urandom(1 << 18), os.urandom(1 << 18)
    buf = bytearray(a)
    view = memoryview(buf).toreadonly()
    stop = threading.Event()

    def flip():
        while not stop.is_set():
            buf[:] = b
            buf[:] = a

    t = threading.Thread(target=flip)
    t.start()
    try:
        for _ in range(20):
            assert roundtrip(view) in (a, b)
    finally:
        stop.set()
        t.join()


def test_truncated_input():
    with pytest.raises(lzallright.LZOError) as exc:
        lzallright.LZOCompressor.decompress(b"\x11\x00")

    assert exc.value.args == (lzallright.EResult.InputOverrun,)


def test_shared_compressor_across_threads():
    c = lzallright.LZOCompressor()
    data = [bytes([i]) * 50_000 + os.urandom(50_000) for i in range(16)]

    with ThreadPoolExecutor(8) as ex:
        outs = list(ex.map(c.compress, data))

    assert outs == [lzallright.LZOCompressor().compress(d) for d in data]
