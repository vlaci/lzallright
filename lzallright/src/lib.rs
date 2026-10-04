//! The crate is started as a port of the C++ [lzokay] library.
//!
//! [lzokay]: https://github.com/jackoalan/lzokay

mod compress;
mod consts;
mod decompress;
mod error;
mod matching;
mod window;

pub use compress::{compress, worst_case_len};
pub use decompress::decompress;
pub use error::{Error, ErrorKind};
pub use matching::Dict;
