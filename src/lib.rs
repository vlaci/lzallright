pub mod error;

#[cfg(not(miri))]
mod lzallright;

#[cfg(not(feature = "lzokay"))]
pub mod lzo;
#[cfg(feature = "lzokay")]
pub mod lzokay;

#[cfg(not(miri))]
mod python;

#[cfg(not(miri))]
pub use crate::lzallright::LZOCompressor;

#[cfg(feature = "lzokay")]
pub use lzokay as backend;

#[cfg(not(feature = "lzokay"))]
pub use lzo as backend;
