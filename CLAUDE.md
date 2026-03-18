# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Architecture

This is a Rust-based Python extension providing LZO compression bindings. The project has three main components:

- **lzokay-sys**: Low-level Rust bindings to the C++ lzokay library using cxx bridge
- **Main crate**: High-level Rust API and Python bindings using PyO3
- **Python package**: Final Python distribution with type stubs

### Key Files

- `src/lzo.rs`: Pure-Rust LZO compress/decompress with unsafe raw pointer arithmetic in decompress_inner
- `src/lzallright.rs`: PyO3 Python class definitions wrapping lzo module
- `src/python.rs`: Buffer handling for Python interop
- `lzokay-sys/src/lib.rs`: FFI bindings to C++ lzokay library using cxx
- `lzokay-sys/wrapper.hpp`: C++ wrapper for lzokay
- `python/lzallright/_lzallright.pyi`: Python type stubs

## Development Commands

### Building and Testing

```bash
# Build the Python extension
maturin develop

# Run Rust tests
cargo test

# Run Python tests
pytest tests/

# Run benchmarks
cargo bench

# Run C++ lzokay tests under AddressSanitizer (requires nightly)
RUSTFLAGS="-Zsanitizer=address" cargo test --features lzokay -Zbuild-std --target x86_64-unknown-linux-gnu -- lzokay::tests

# Run tests under Miri (detects undefined behavior in unsafe code)
cargo miri test

# Run fuzz testing (requires nightly)
cargo fuzz run decompress -- -max_total_time=30

# Format code
cargo fmt
ruff format

# Lint code
cargo clippy
ruff check

# Type check Python code
mypy
```

### Using Nix

The project supports Nix-based development:

```bash
# Enter development shell
nix develop

# Build package
nix build

# Run checks
nix flake check
```

### Publishing

```bash
# Build Python wheels
maturin build --release

# Update changelog
towncrier build --version X.Y.Z
```

## Architecture Notes

- Nix devshell provides nightly Rust toolchain (via rust-overlay) with Miri and cargo-fuzz
- Uses `maturin` to build Python extensions from Rust
- C++ lzokay library is statically linked via cmake
- Supports Python 3.8+ with stable ABI (abi3)
- Thread-safe compression with per-instance dictionaries
- Buffer protocol support for efficient Python interop
- Custom exception hierarchy: `LZOError` and `InputNotConsumed`

## Key Dependencies

- arbtest + arbitrary: Property-based testing
- libfuzzer-sys: Coverage-guided fuzzing (in `fuzz/`)
- PyO3: Python bindings
- cxx: C++ interop
- maturin: Python extension builder
- lzokay: C++ LZO implementation (git submodule)