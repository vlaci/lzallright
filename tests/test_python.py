import array
import mmap

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


def test_decompress_truncated(lorem):
    comp = lzallright.LZOCompressor().compress(lorem)

    with pytest.raises(lzallright.LZOError) as exc:
        lzallright.LZOCompressor.decompress(comp[:-10])

    assert exc.value.args == (lzallright.EResult.InputOverrun,)


def _mmap(data):
    m = mmap.mmap(-1, len(data))
    m.write(data)
    return m


@pytest.mark.parametrize(
    "wrap",
    [bytearray, memoryview, _mmap, lambda d: array.array("B", d)],
    ids=["bytearray", "memoryview", "mmap", "array-B"],
)
def test_buffer_types(lorem, wrap):
    comp = lzallright.LZOCompressor().compress(wrap(lorem))
    assert lzallright.LZOCompressor.decompress(wrap(comp)) == lorem


def test_buffer_non_byte_items():
    data = array.array("i", range(1000))
    comp = lzallright.LZOCompressor().compress(data)
    assert lzallright.LZOCompressor.decompress(comp) == data.tobytes()


@pytest.mark.parametrize("data", ["str", [1, 2, 3], 5], ids=["str", "list", "int"])
def test_non_buffer_rejected(data):
    with pytest.raises(TypeError):
        lzallright.LZOCompressor().compress(data)
    with pytest.raises(TypeError):
        lzallright.LZOCompressor.decompress(data)
