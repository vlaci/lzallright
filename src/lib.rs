mod lzallright;
pub mod lzo;
#[cfg(feature = "lzokay")]
mod lzokay;
mod python;

#[cfg(not(feature = "lzokay"))]
pub(crate) use lzo as backend;
#[cfg(feature = "lzokay")]
pub(crate) use lzokay as backend;

pub use crate::lzallright::LZOCompressor;
