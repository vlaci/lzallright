#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

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
