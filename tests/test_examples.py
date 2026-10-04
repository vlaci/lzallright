import runpy
from pathlib import Path

import pytest

EXAMPLES = sorted((Path(__file__).parent / "examples").glob("*.py"))


@pytest.mark.parametrize("example", EXAMPLES, ids=lambda e: e.name)
def test_examples(example: Path):
    runpy.run_path(str(example))
