#[cxx::bridge(namespace = "lzokay")]
mod lzokay {
    #[repr(i32)]
    pub enum EResult {
        LookbehindOverrun = -4,
        OutputOverrun,
        InputOverrun,
        Error,
        Success,
        InputNotConsumed,
    }

    unsafe extern "C++" {
        include!("lzokay-sys/wrapper.hpp");

        type EResult;
        type DictBase;

        /// Compresses data
        ///
        /// # Safety
        ///
        /// - `src` and `dst` has to be initialized to be at least `src_size` and `dst_size`.
        /// - `dict` has to be created by `new_dict`
        unsafe fn compress(
            src: *const u8,
            src_size: usize,
            dst: *mut u8,
            dst_size: usize,
            out_size: &mut usize,
            dict: Pin<&mut DictBase>,
        ) -> EResult;

        /// Decompresses data
        ///
        /// # Safety
        ///
        /// `src` and `dst` has to be initialized to be at least `src_size` and `dst_size`.
        unsafe fn decompress(
            src: *const u8,
            src_size: usize,
            dst: *mut u8,
            dst_size: usize,
            out_size: &mut usize,
        ) -> EResult;

        fn new_dict() -> UniquePtr<DictBase>;
    }
}
pub use lzokay::*;

unsafe impl Send for DictBase {}
