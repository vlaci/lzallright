#![allow(non_upper_case_globals)]
pub const HASH_SIZE: usize = 0x4000;
pub const MAX_DIST: usize = 0xbfff;
pub const MAX_MATCH_LEN: usize = 0x800;
pub const BUF_SIZE: usize = MAX_DIST + MAX_MATCH_LEN;

pub const M1MaxOffset: usize = 0x0400;
pub const M2MaxOffset: usize = 0x0800;
pub const M3MaxOffset: usize = 0x4000;
// pub const M4MaxOffset: usize = 0xbfff;

// pub const M1MinLen: usize = 2;
// pub const M1MaxLen: usize = 2;
pub const M2MinLen: usize = 3;
pub const M2MaxLen: usize = 8;
// pub const M3MinLen: usize = 3;
pub const M3MaxLen: usize = 33;
// pub const M4MinLen: usize = 3;
pub const M4MaxLen: usize = 9;

pub const M1Marker: usize = 0x0;
// pub const M2Marker: usize = 0x40;
pub const M3Marker: usize = 0x20;
pub const M4Marker: usize = 0x10;

pub const MaxMatchByLengthLen: usize = 34; /* Max M3 len + 1 */

pub const Max255Count: usize = !0usize / 255 - 2;
