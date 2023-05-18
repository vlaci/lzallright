import lzallright
import pytest


@pytest.fixture
def lorem(request):
    return (request.session.path / "benches/lorem.txt").read_bytes()


def test_roundtrip(lorem):
    c = lzallright.LZOCompressor()
    comp = c.compress(lorem)

    assert lzallright.LZOCompressor.decompress(comp) == lorem


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
