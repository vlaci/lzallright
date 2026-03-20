import lzo
import pytest

import lzallright


@pytest.fixture
def lorem(request):
    return (request.session.path / "benches/lorem.txt").read_bytes()


def test_lzo_compressed_data_decompressible_by_lzallright(lorem):
    include_header = False
    compressed = lzo.compress(lorem, 1, include_header)
    assert lzallright.LZOCompressor.decompress(compressed) == lorem


def test_lzallright_compressed_data_decompressible_by_lzo(lorem):
    include_header = False
    c = lzallright.LZOCompressor()
    compressed = c.compress(lorem)
    assert lzo.decompress(compressed, include_header, len(lorem)) == lorem
