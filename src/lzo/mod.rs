//! Safe Rust LZO codec, ported from the C++ [lzokay] library.
//!
//! [lzokay]: https://github.com/jackoalan/lzokay

mod compress;
mod consts;
mod decompress;
mod matching;
mod window;

pub use compress::compress;
pub use decompress::decompress;
pub use matching::Dict;
