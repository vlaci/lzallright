"""Training workload for PGO builds (`pgo-command` in pyproject.toml).

Runs the instrumented extension over the benchmark corpus so the profile
reflects the inputs `cargo bench` measures. Exits non-zero if any input
fails to round-trip, which aborts the PGO build.
"""

import sys
from pathlib import Path

from lzallright import LZOCompressor

CORPUS = Path(__file__).resolve().parent.parent / "benches" / "corpus"
CORPUS_FILES = (
    "alice-pg11.txt",
    "sqlite-btree.c",
    "wikidata-Q100020.json",
    "nasa-http-jul95.log",
)
RANDOM_LEN = 8192
ROUNDS = 200
MASK64 = (1 << 64) - 1


def pseudo_random_bytes(length: int) -> bytes:
    """Mirror `pseudo_random_bytes` in benches/compress.rs (xorshift64)."""
    x = 0x9E3779B97F4A7C15
    out = bytearray(length)
    for i in range(length):
        x ^= (x << 13) & MASK64
        x ^= x >> 7
        x ^= (x << 17) & MASK64
        out[i] = x & 0xFF
    return bytes(out)


def main() -> None:
    """Compress and decompress every input ROUNDS times, verifying round-trips."""
    inputs = [(name, (CORPUS / name).read_bytes()) for name in CORPUS_FILES]
    inputs.append(("random", pseudo_random_bytes(RANDOM_LEN)))
    for name, data in inputs:
        compressor = LZOCompressor()
        for _ in range(ROUNDS):
            packed = compressor.compress(data)
            if LZOCompressor.decompress(packed, output_size_hint=len(data)) != data:
                sys.exit(f"pgo_profile: round-trip mismatch for {name}")


if __name__ == "__main__":
    main()
