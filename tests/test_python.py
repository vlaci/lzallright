import lzallright
import pytest


@pytest.fixture
def lorem(request):
    return (request.session.path / "benches/lorem.txt").read_bytes()


def test_roundtrip(lorem):
    c = lzallright.LZOCompressor()
    comp = c.compress(lorem)

    assert lzallright.LZOCompressor.decompress(comp) == lorem
