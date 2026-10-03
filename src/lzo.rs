#![allow(non_upper_case_globals)]
use crate::error::{Error, ErrorKind};
use std::cell::Cell;

const HASH_SIZE: usize = 0x4000;
const MAX_DIST: usize = 0xbfff;
const MAX_MATCH_LEN: usize = 0x800;
const BUF_SIZE: usize = MAX_DIST + MAX_MATCH_LEN;

const M1MaxOffset: usize = 0x0400;
const M2MaxOffset: usize = 0x0800;
const M3MaxOffset: usize = 0x4000;
// const M4MaxOffset: usize = 0xbfff;

// const M1MinLen: usize = 2;
// const M1MaxLen: usize = 2;
const M2MinLen: usize = 3;
const M2MaxLen: usize = 8;
// const M3MinLen: usize = 3;
const M3MaxLen: usize = 33;
// const M4MinLen: usize = 3;
const M4MaxLen: usize = 9;

const M1Marker: usize = 0x0;
// const M2Marker: usize = 0x40;
const M3Marker: usize = 0x20;
const M4Marker: usize = 0x10;

const MaxMatchByLengthLen: usize = 34; /* Max M3 len + 1 */

struct Match3 {
    head: [u16; HASH_SIZE],
    chain_sz: [u16; HASH_SIZE],
    chain: [u16; BUF_SIZE],
    best_len: [u16; BUF_SIZE],
}

impl Match3 {
    fn make_key(data: &[u8]) -> usize {
        let intermediate = ((usize::from(data[0]) << 5) ^ usize::from(data[1])).wrapping_shl(5)
            ^ usize::from(data[2]);

        intermediate.wrapping_mul(0x9F5F).wrapping_shr(5) & 0x3FFF
    }
    const fn get_head(&self, key: usize) -> u16 {
        if self.chain_sz[key] == 0 {
            u16::MAX
        } else {
            self.head[key]
        }
    }
    fn remove(&mut self, pos: usize, b: &[u8]) {
        let c = &mut self.chain_sz[Self::make_key(&b[pos..])];
        *c = c.wrapping_sub(1);
    }

    fn advance(&mut self, s: &State, b: &[u8]) -> (usize, usize) {
        let key = Self::make_key(&b[s.wind_b..]);
        let head = self.get_head(key);
        let match_pos = head;
        self.chain[s.wind_b] = head;
        let mut match_count = self.chain_sz[key];
        self.chain_sz[key] += 1;
        if match_count > MAX_MATCH_LEN as u16 {
            match_count = MAX_MATCH_LEN as u16
        }
        self.head[key] = s.wind_b as u16;
        (match_pos as usize, match_count as usize)
    }

    fn skip_advance(&mut self, s: &State, b: &[u8]) {
        let key = Self::make_key(&b[s.wind_b..]);
        self.chain[s.wind_b] = self.get_head(key);
        self.head[key] = s.wind_b as _;
        self.best_len[s.wind_b] = MAX_MATCH_LEN as u16 + 1;
        let c = &mut self.chain_sz[key];
        *c = c.wrapping_add(1);
    }

    fn reset(&mut self) {
        self.chain_sz.fill(0);
    }
}

impl Default for Match3 {
    fn default() -> Self {
        Self {
            head: [0; HASH_SIZE],
            chain_sz: [0; HASH_SIZE],
            chain: [0; BUF_SIZE],
            best_len: [0; BUF_SIZE],
        }
    }
}

struct Match2 {
    head: [u16; 1 << 16],
}

impl Match2 {
    fn make_key(data: &[u8]) -> usize {
        (data[0] as u16 ^ ((data[1] as u16) << 8)) as usize
    }

    fn add(&mut self, pos: u16, b: &[u8]) {
        self.head[Self::make_key(&b[pos as _..])] = pos;
    }
    fn remove(&mut self, pos: usize, b: &[u8]) {
        let p = &mut self.head[Self::make_key(&b[pos as _..])];
        if *p == pos as u16 {
            *p = u16::MAX
        }
    }

    fn search(&self, s: &State, b: &[u8]) -> Option<usize> {
        let pos = self.head[Self::make_key(&b[s.wind_b..])];
        if pos == u16::MAX {
            return None;
        }
        Some(usize::from(pos))
    }

    fn reset(&mut self) {
        self.head.fill(u16::MAX);
    }
}

impl Default for Match2 {
    fn default() -> Self {
        Self {
            head: [u16::MAX; 1 << 16],
        }
    }
}

struct State<'a> {
    src: &'a [u8],
    inp: usize,
    wind_sz: usize,
    wind_b: usize,
    wind_e: usize,
    cycle1_countdown: usize,
    bufp: usize,
    buf_sz: usize,
}

impl<'a> State<'a> {
    fn new(src: &'a [u8], buf: &mut Window) -> Self {
        let wind_sz = src.len().min(MAX_MATCH_LEN);
        let mut state = State {
            src,
            inp: wind_sz,
            wind_sz,
            wind_b: 0,
            wind_e: wind_sz,
            cycle1_countdown: MAX_DIST,
            bufp: 0,
            buf_sz: 0,
        };

        buf[..wind_sz].copy_from_slice(&src[..wind_sz]);

        // Unreachable under current settings
        // assert!(MAX_MATCH_LEN < BUF_SIZE);
        if state.wind_e == BUF_SIZE {
            state.wind_e = 0;
        }

        if wind_sz < 3 {
            buf[wind_sz..wind_sz + 3].fill(0);
        }
        state
    }

    /// Access next input byte and advance both ends of circular buffer
    fn get_byte(&mut self, buf: &mut Window) {
        let b = match self.src.get(self.inp) {
            Some(&b) => {
                self.inp += 1;
                b
            }
            None => {
                self.wind_sz = self.wind_sz.saturating_sub(1);
                0
            }
        };
        buf[self.wind_e] = b;

        if self.wind_e < MAX_MATCH_LEN {
            buf[BUF_SIZE + self.wind_e] = b;
        }

        self.wind_e += 1;
        if self.wind_e == BUF_SIZE {
            self.wind_e = 0;
        }

        self.wind_b += 1;
        if self.wind_b == BUF_SIZE {
            self.wind_b = 0;
        }
    }

    fn pos2off(&self, pos: usize) -> usize {
        if self.wind_b > pos {
            self.wind_b - pos
        } else {
            BUF_SIZE - (pos - self.wind_b)
        }
    }
}

/// Best lookback match found at the current position.
struct LookbackMatch {
    len: usize,
    off: usize,
    best_off: [usize; MaxMatchByLengthLen],
}

impl LookbackMatch {
    fn new() -> Self {
        Self {
            len: 0,
            off: 0,
            best_off: [0; MaxMatchByLengthLen],
        }
    }

    fn find_better_match(&mut self) {
        if self.len <= M2MinLen || self.off <= M2MaxOffset {
            return;
        }
        if self.len <= M2MaxLen + 1
            && self.best_off[self.len - 1] != 0
            && self.best_off[self.len - 1] <= M2MaxOffset
        {
            self.len -= 1;
        } else if self.off > M3MaxOffset
            && self.len > M4MaxLen
            && self.len <= M2MaxLen + 2
            && self.best_off[self.len - 2] != 0
            && self.best_off[self.len] <= M2MaxOffset
        {
            self.len -= 2;
        } else if self.off > M3MaxOffset
            && self.len > M4MaxLen
            && self.len <= M3MaxLen + 1
            && self.best_off[self.len - 1] != 0
            && self.best_off[self.len - 2] <= M3MaxOffset
        {
            self.len -= 1;
        } else {
            return;
        }
        self.off = self.best_off[self.len];
    }
}

/// Circular buffer caching enough data to access the maximum lookback
/// distance of 48K + maximum match length of 2K. An additional 2K is
/// allocated so the start of the buffer may be replicated at the end,
/// therefore providing efficient circular access.
type Window = [u8; BUF_SIZE + MAX_MATCH_LEN];

pub struct Dict {
    match3: Match3,
    match2: Match2,
    buffer: Window,
}

impl Default for Dict {
    fn default() -> Self {
        Self {
            match3: Default::default(),
            match2: Default::default(),
            buffer: [0; BUF_SIZE + MAX_MATCH_LEN],
        }
    }
}

impl Dict {
    pub fn new() -> Self {
        Default::default()
    }

    fn reset(&mut self) {
        self.match3.reset();
        self.match2.reset();
    }

    fn reset_next_input_entry(&mut self, s: &mut State) {
        // Remove match from about-to-be-clobbered buffer entry
        if s.cycle1_countdown == 0 {
            self.match3.remove(s.wind_e, &self.buffer);
            self.match2.remove(s.wind_e, &self.buffer);
        } else {
            s.cycle1_countdown -= 1;
        }
    }

    fn advance(&mut self, s: &mut State, lb: &mut LookbackMatch, skip: bool) {
        if skip {
            for _ in 1..lb.len {
                self.reset_next_input_entry(s);
                self.match3.skip_advance(s, &self.buffer);
                self.match2.add(s.wind_b as u16, &self.buffer);
                s.get_byte(&mut self.buffer);
            }
        }

        lb.len = 1;
        lb.off = 0;

        let (mut match_pos, match_count) = self.match3.advance(s, &self.buffer);
        let best_len = lb.len;
        let at_end = s.wind_sz == 0;

        if s.wind_sz <= 1 {
            self.match3.best_len[s.wind_b] = MAX_MATCH_LEN as u16 + 1;
        } else {
            let mut best_pos = [0; MaxMatchByLengthLen];
            if let Some(p) = self.match2.search(s, &self.buffer) {
                best_pos[2] = p + 1;
                lb.len = 2;
                let mut lb_pos = p;
                if s.wind_sz >= 3 {
                    let wind = &self.buffer[s.wind_b..s.wind_b + s.wind_sz];
                    for _ in 0..match_count {
                        let match_len =
                            mismatch(wind, &self.buffer[match_pos..match_pos + s.wind_sz]);

                        if match_len < 2 {
                            match_pos = self.match3.chain[match_pos] as usize;
                            continue;
                        }
                        if match_len < MaxMatchByLengthLen && best_pos[match_len] == 0 {
                            best_pos[match_len] = match_pos + 1;
                        }
                        if match_len > lb.len {
                            lb.len = match_len;
                            lb_pos = match_pos;
                            if match_len == s.wind_sz
                                || match_len > self.match3.best_len[match_pos] as usize
                            {
                                break;
                            }
                        }
                        match_pos = self.match3.chain[match_pos] as usize;
                    }
                }
                if lb.len > best_len {
                    lb.off = s.pos2off(lb_pos);
                }
            }
            self.match3.best_len[s.wind_b] = lb.len as u16;
            for (off, &pos) in lb.best_off[2..].iter_mut().zip(&best_pos[2..]) {
                *off = if pos > 0 {
                    s.pos2off(pos.wrapping_sub(1))
                } else {
                    0
                };
            }
        }

        self.reset_next_input_entry(s);

        self.match2.add(s.wind_b as u16, &self.buffer);

        s.get_byte(&mut self.buffer);

        if at_end {
            s.buf_sz = 0;
            lb.len = 0
            /* Signal exit */
        } else {
            s.buf_sz = s.wind_sz + 1;
        }
        s.bufp = s.inp - s.buf_sz;
    }
}

/// Length of common prefix of `a` and `b`
#[inline(always)]
fn mismatch(a: &[u8], b: &[u8]) -> usize {
    const WORD_SIZE: usize = std::mem::size_of::<u64>();
    let n = a.len().min(b.len());
    let a = &a[..n];
    let b = &b[..n];
    let mut i = 0;
    for (ca, cb) in a.chunks_exact(WORD_SIZE).zip(b.chunks_exact(WORD_SIZE)) {
        let diff =
            u64::from_le_bytes(ca.try_into().unwrap()) ^ u64::from_le_bytes(cb.try_into().unwrap());
        if diff != 0 {
            return i + diff.trailing_zeros() as usize / 8;
        }
        i += WORD_SIZE;
    }
    a[i..]
        .iter()
        .zip(&b[i..])
        .position(|(x, y)| x != y)
        .map_or(n, |p| i + p)
}

const Max255Count: usize = !0usize / 255 - 2;

struct Decoder<'a> {
    src: &'a [u8],
    inp: usize,
    dst: &'a mut [u8],
    outp: usize,
}

impl<'a> Decoder<'a> {
    fn new(src: &'a [u8], dst: &'a mut [u8]) -> Self {
        Self {
            src,
            inp: 0,
            dst,
            outp: 0,
        }
    }

    #[inline]
    fn error(&self, kind: ErrorKind) -> Error {
        Error::new(kind, self.outp)
    }

    #[inline]
    fn needs_in(&self, count: usize) -> Result<(), Error> {
        if count > self.src.len() - self.inp {
            Err(self.error(ErrorKind::InputOverrun))
        } else {
            Ok(())
        }
    }

    #[inline]
    fn needs_out(&self, count: usize) -> Result<(), Error> {
        if count > self.dst.len() - self.outp {
            Err(self.error(ErrorKind::OutputOverrun))
        } else {
            Ok(())
        }
    }

    #[inline]
    fn read_byte(&mut self) -> Result<usize, Error> {
        let byte = *self
            .src
            .get(self.inp)
            .ok_or_else(|| self.error(ErrorKind::InputOverrun))?;
        self.inp += 1;
        Ok(usize::from(byte))
    }

    #[inline]
    fn read_le16(&mut self) -> Result<usize, Error> {
        self.needs_in(2)?;
        let v = u16::from_le_bytes([self.src[self.inp], self.src[self.inp + 1]]);
        self.inp += 2;
        Ok(usize::from(v))
    }

    /// Variable length encoding: `base + (zero_bytes * 255) + non_zero_byte`
    #[inline]
    fn read_zero_byte_length(&mut self, base: usize) -> Result<usize, Error> {
        let zeros = self.src[self.inp..].iter().take_while(|&&b| b == 0).count();
        self.inp += zeros;
        if zeros > Max255Count {
            return Err(self.error(ErrorKind::Error));
        }
        Ok(zeros * 255 + base + self.read_byte()?)
    }

    #[inline]
    fn copy_literal(&mut self, len: usize) -> Result<(), Error> {
        self.needs_in(len)?;
        self.needs_out(len)?;
        self.dst[self.outp..self.outp + len].copy_from_slice(&self.src[self.inp..self.inp + len]);
        self.inp += len;
        self.outp += len;
        Ok(())
    }

    /// Copies `len` bytes from `dist` bytes behind the output position, then
    /// `nstate` literals.
    #[inline]
    fn copy_match(&mut self, dist: usize, len: usize, nstate: usize) -> Result<(), Error> {
        if dist > self.outp {
            return Err(self.error(ErrorKind::LookbehindOverrun));
        }
        self.needs_in(nstate)?;
        self.needs_out(len + nstate)?;

        // cannot use copy_within as RLE needs byte-wise copy
        let window = &mut self.dst[self.outp - dist..self.outp + len];
        let window = Cell::from_mut(window).as_slice_of_cells();
        if len <= 8 {
            // PERF: short copy optimization
            for k in 0..8 {
                if k < len {
                    window[dist + k].set(window[k].get());
                }
            }
        } else {
            for (d, s) in window[dist..].iter().zip(window) {
                d.set(s.get());
            }
        }
        self.outp += len;

        // `nstate` is at most 3: a guarded, fully unrolled copy beats the
        // out-of-line `memcpy` call LLVM emits for a plain copy loop.
        let out = &mut self.dst[self.outp..self.outp + nstate];
        let lit = &self.src[self.inp..self.inp + nstate];
        for k in 0..3 {
            if k < nstate {
                out[k] = lit[k];
            }
        }
        self.inp += nstate;
        self.outp += nstate;
        Ok(())
    }
}

pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    if src.len() < 3 {
        return Err(Error::new(ErrorKind::InputOverrun, 0));
    }

    let mut decoder = Decoder::new(src, dst);

    let mut lbdist;
    let mut lblen;
    let mut state = 0;
    let mut nstate;

    let first = src[0] as usize;

    // First byte encoding
    if first >= 22 {
        // 22..255 : copy literal string
        //           length = (byte - 17) = 4..238
        //           state = 4 [ don't copy extra literals ]
        //           skip byte
        decoder.inp += 1;
        decoder.copy_literal(first - 17)?;
        state = 4;
    } else if first >= 18 {
        // 18..21 : copy 0..3 literals
        //          state = (byte - 17) = 0..3  [ copy <state> literals ]
        //          skip byte
        decoder.inp += 1;
        state = first - 17;
        decoder.copy_literal(state)?;
    }

    // 0..17 : follow regular instruction encoding, see below. It is worth
    //         noting that codes 16 and 17 will represent a block copy from
    //         the dictionary which is empty, and that they will always be
    //         invalid at this place.

    loop {
        let inst = decoder.read_byte()?;
        if inst & 0xC0 != 0 {
            // [M2]
            // 1 L L D D D S S  (128..255)
            //   Copy 5-8 bytes from block within 2kB distance
            //   state = S (copy S literals after this block)
            //   length = 5 + L
            // Always followed by exactly one byte : H H H H H H H H
            //   distance = (H << 3) + D + 1
            //
            // 0 1 L D D D S S  (64..127)
            //   Copy 3-4 bytes from block within 2kB distance
            //   state = S (copy S literals after this block)
            //   length = 3 + L
            // Always followed by exactly one byte : H H H H H H H H
            //   distance = (H << 3) + D + 1
            let b = decoder.read_byte()?;
            lbdist = (b << 3) + ((inst >> 2) & 0x7) + 1;
            lblen = (inst >> 5) + 1;
            nstate = inst & 0x3;
        } else if inst & M3Marker != 0 {
            // [M3]
            // 0 0 1 L L L L L  (32..63)
            //   Copy of small block within 16kB distance (preferably less than 34B)
            //   length = 2 + (L ?: 31 + (zero_bytes * 255) + non_zero_byte)
            // Always followed by exactly one LE16 :  D D D D D D D D : D D D D D D S S
            //   distance = D + 1
            //   state = S (copy S literals after this block)
            lblen = (inst & 0x1f) + 2;
            if lblen == 2 {
                let offset = decoder.read_zero_byte_length(31)?;
                lblen += offset;
            }
            nstate = decoder.read_le16()?;
            lbdist = (nstate >> 2) + 1;
            nstate &= 0x3;
        } else if inst & M4Marker != 0 {
            // [M4]
            // 0 0 0 1 H L L L  (16..31)
            //   Copy of a block within 16..48kB distance (preferably less than 10B)
            //   length = 2 + (L ?: 7 + (zero_bytes * 255) + non_zero_byte)
            // Always followed by exactly one LE16 :  D D D D D D D D : D D D D D D S S
            //   distance = 16384 + (H << 14) + D
            //   state = S (copy S literals after this block)
            //   End of stream is reached if distance == 16384
            lblen = (inst & 0x7) + 2;
            if lblen == 2 {
                let offset = decoder.read_zero_byte_length(7)?;
                lblen += offset;
            }
            nstate = decoder.read_le16()?;
            lbdist = ((inst & 0x8) << 11) + (nstate >> 2);
            nstate &= 0x3;
            if lbdist == 0 {
                break; /* Stream finished */
            }
            lbdist += 16384;
        } else {
            // [M1] Depends on the number of literals copied by the last instruction. */
            if state == 0 {
                // If last instruction did not copy any literal (state == 0), this
                // encoding will be a copy of 4 or more literal, and must be interpreted
                // like this :
                //
                //    0 0 0 0 L L L L  (0..15)  : copy long literal string
                //    length = 3 + (L ?: 15 + (zero_bytes * 255) + non_zero_byte)
                //    state = 4  (no extra literals are copied)
                let mut len = inst + 3;
                if len == 3 {
                    let offset = decoder.read_zero_byte_length(15)?;
                    len += offset;
                }
                decoder.copy_literal(len)?;
                state = 4;
                continue;
            } else if state != 4 {
                // If last instruction used to copy between 1 to 3 literals (encoded in
                // the instruction's opcode or distance), the instruction is a copy of a
                // 2-byte block from the dictionary within a 1kB distance. It is worth
                // noting that this instruction provides little savings since it uses 2
                // bytes to encode a copy of 2 other bytes but it encodes the number of
                // following literals for free. It must be interpreted like this :
                //
                //    0 0 0 0 D D S S  (0..15)  : copy 2 bytes from <= 1kB distance
                //    length = 2
                //    state = S (copy S literals after this block)
                //  Always followed by exactly one byte : H H H H H H H H
                //    distance = (H << 2) + D + 1
                let b = decoder.read_byte()?;
                nstate = inst & 0x3;
                lbdist = (inst >> 2) + (b << 2) + 1;
                lblen = 2;
            } else {
                // If last instruction used to copy 4 or more literals (as detected by
                // state == 4), the instruction becomes a copy of a 3-byte block from the
                // dictionary from a 2..3kB distance, and must be interpreted like this :
                //
                //    0 0 0 0 D D S S  (0..15)  : copy 3 bytes from 2..3 kB distance
                //    length = 3
                //    state = S (copy S literals after this block)
                //  Always followed by exactly one byte : H H H H H H H H
                //    distance = (H << 2) + D + 2049
                let b = decoder.read_byte()?;
                nstate = inst & 0x3;
                lbdist = (inst >> 2) + (b << 2) + 2049;
                lblen = 3;
            }
        }
        decoder.copy_match(lbdist, lblen, nstate)?;
        state = nstate;
    }

    if lblen != 3 {
        // Ensure terminating M4 was encountered
        return Err(decoder.error(ErrorKind::Error));
    }
    if decoder.inp == src.len() {
        Ok(decoder.outp)
    } else {
        Err(decoder.error(ErrorKind::InputNotConsumed))
    }
}

struct Writer<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> Writer<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    #[inline]
    fn reserve(&self, count: usize) -> Result<(), Error> {
        if count > self.buf.len() - self.pos {
            Err(Error::new(ErrorKind::OutputOverrun, self.pos))
        } else {
            Ok(())
        }
    }

    #[inline]
    fn write_byte(&mut self, byte: u8) {
        self.buf[self.pos] = byte;
        self.pos += 1;
    }

    fn write_zero_byte_length(&mut self, mut len: usize) {
        while len > 255 {
            self.write_byte(0);
            len -= 255;
        }
        self.write_byte(len as u8);
    }

    fn encode_literal_run(&mut self, lit: &[u8]) -> Result<(), Error> {
        let lit_len = lit.len();
        if self.pos == 0 && lit_len <= 238 {
            self.reserve(1)?;
            self.write_byte(17 + lit_len as u8);
        } else if lit_len <= 3 {
            self.buf[self.pos - 2] |= lit_len as u8;
        } else if lit_len <= 18 {
            self.reserve(1)?;
            self.write_byte(lit_len as u8 - 3);
        } else {
            self.reserve((lit_len - 18) / 255 + 2)?;
            self.write_byte(0);
            self.write_zero_byte_length(lit_len - 18);
        }
        self.reserve(lit_len)?;
        self.buf[self.pos..self.pos + lit_len].copy_from_slice(lit);
        self.pos += lit_len;
        Ok(())
    }

    fn encode_lookback_match(
        &mut self,
        mut lb_len: usize,
        mut lb_off: usize,
        last_lit_len: usize,
    ) -> Result<(), Error> {
        if lb_len == 2 {
            lb_off -= 1;
            self.reserve(2)?;
            self.write_byte((M1Marker | ((lb_off & 0x3) << 2)) as u8);
            self.write_byte((lb_off >> 2) as u8);
        } else if lb_len <= M2MaxLen && lb_off <= M2MaxOffset {
            lb_off -= 1;
            self.reserve(2)?;
            self.write_byte(((lb_len - 1) << 5 | ((lb_off & 0x7) << 2)) as u8);
            self.write_byte((lb_off >> 3) as u8);
        } else if lb_len == M2MinLen && lb_off <= M1MaxOffset + M2MaxOffset && last_lit_len >= 4 {
            lb_off -= 1 + M2MaxOffset;
            self.reserve(2)?;
            self.write_byte((M1Marker | ((lb_off & 0x3) << 2)) as u8);
            self.write_byte((lb_off >> 2) as u8);
        } else {
            let (marker, max_len) = if lb_off <= M3MaxOffset {
                lb_off -= 1;
                (M3Marker, M3MaxLen)
            } else {
                lb_off -= 0x4000;
                (M4Marker | ((lb_off & 0x4000) >> 11), M4MaxLen)
            };
            if lb_len <= max_len {
                self.reserve(1)?;
                self.write_byte((marker | (lb_len - 2)) as u8);
            } else {
                lb_len -= max_len;
                self.reserve(lb_len / 255 + 2)?;
                self.write_byte(marker as u8);
                self.write_zero_byte_length(lb_len);
            }
            self.reserve(2)?;
            self.write_byte((lb_off << 2) as u8);
            self.write_byte((lb_off >> 6) as u8);
        }
        Ok(())
    }
}

pub fn compress(src: &[u8], out: &mut [u8], dict: &mut Dict) -> Result<usize, Error> {
    dict.reset();
    let mut s = State::new(src, &mut dict.buffer);
    let mut writer = Writer::new(out);
    let mut lit_len = 0;
    let mut lit_pos = s.inp;

    let mut lb = LookbackMatch::new();
    dict.advance(&mut s, &mut lb, false);

    while s.buf_sz > 0 {
        if lit_len == 0 {
            lit_pos = s.bufp;
        }
        #[allow(clippy::if_same_then_else)]
        if lb.len < 2
            || (lb.len == 2 && (lb.off > M1MaxOffset || lit_len == 0 || lit_len >= 4))
            || (lb.len == 2 && writer.pos == 0)
            || (writer.pos == 0 && lit_len == 0)
        {
            lb.len = 0;
        } else if lb.len == M2MinLen && lb.off > M1MaxOffset + M2MaxOffset && lit_len >= 4 {
            lb.len = 0;
        }
        if lb.len == 0 {
            lit_len += 1;
            dict.advance(&mut s, &mut lb, false);
            continue;
        }
        lb.find_better_match();
        writer.encode_literal_run(&src[lit_pos..lit_pos + lit_len])?;
        writer.encode_lookback_match(lb.len, lb.off, lit_len)?;
        lit_len = 0;
        dict.advance(&mut s, &mut lb, true);
    }

    writer.encode_literal_run(&src[lit_pos..lit_pos + lit_len])?;

    /* Terminating M4 */
    writer.reserve(3)?;
    writer.write_byte((M4Marker | 1) as u8);
    writer.write_byte(0);
    writer.write_byte(0);

    Ok(writer.pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatch_returns_end_position_for_matching_slices() {
        assert_eq!(mismatch(b"aaaa", b"aaaa"), 4);
    }

    #[test]
    fn mismatch_returns_smaller_end_when_one_is_a_prefix_of_the_other() {
        assert_eq!(mismatch(b"aaaa", b"aaaaaa"), 4);
        assert_eq!(mismatch(b"aaaaaa", b"aaaa"), 4);
    }

    #[test]
    fn mismatch_returns_position_of_first_diverging_byte() {
        assert_eq!(mismatch(b"aaaa0", b"aaaa1"), 4);
        assert_eq!(mismatch(b"0aaaa", b"1aaaa"), 0);
    }

    #[test]
    fn mismatch_returns_zero_for_empty_slices() {
        assert_eq!(mismatch(b"", b""), 0);
        assert_eq!(mismatch(b"", b"aaaa"), 0);
        assert_eq!(mismatch(b"aaaa", b""), 0);
    }

    #[test]
    fn mismatch_returns_position_at_word_boundaries() {
        for (len, pos) in [(8, 7), (16, 8), (16, 15), (24, 16)] {
            let (a, b) = slices_diverging_at(len, pos);
            assert_eq!(mismatch(&a, &b), pos);
        }
    }

    fn slices_diverging_at(len: usize, pos: usize) -> (Vec<u8>, Vec<u8>) {
        let a = vec![b'a'; len];
        let mut b = a.clone();
        b[pos] += 1;
        (a, b)
    }

    #[test]
    fn mismatch_returns_position_of_divergence_in_tail_after_full_words() {
        for (len, pos) in [(9, 8), (10, 9), (17, 16)] {
            let (a, b) = slices_diverging_at(len, pos);
            assert_eq!(mismatch(&a, &b), pos);
        }
    }

    #[test]
    fn mismatch_returns_first_diverging_byte_when_multiple_bytes_differ() {
        assert_eq!(mismatch(b"aXYZefgh", b"aabcefgh"), 1);
    }

    #[test]
    fn truncated_input_returns_input_overrun() {
        // Opcode 22 declares five literal bytes; only two follow.
        let err = decompress(&[22, 1, 2], &mut [0; 5]).unwrap_err();

        assert_eq!(err.kind(), &ErrorKind::InputOverrun);
        assert_eq!(err.dst_size(), 0);
    }

    #[test]
    fn literal_respects_output_capacity() {
        // Five literal bytes followed by the terminating M4 marker.
        let input = [22, 1, 2, 3, 4, 5, 17, 0, 0];

        for capacity in 0..5 {
            let mut output = vec![0; capacity];
            let err = decompress(&input, &mut output).unwrap_err();

            assert_eq!(err.kind(), &ErrorKind::OutputOverrun);
            assert_eq!(err.dst_size(), 0);
        }

        let mut output = [0; 5];
        assert_eq!(decompress(&input, &mut output).unwrap(), 5);
        assert_eq!(output, [1, 2, 3, 4, 5]);
    }

    #[test]
    fn compressor_respects_output_capacity() {
        let mut dict = Dict::new();

        // Respectively fails while writing the literal header, literal, and
        // terminating M4 marker.
        for capacity in 0..=2 {
            let mut output = vec![0; capacity];
            let err = compress(b"x", &mut output, &mut dict).unwrap_err();

            assert_eq!(err.kind(), &ErrorKind::OutputOverrun);
            assert_eq!(err.dst_size(), capacity);
        }
    }

    #[test]
    fn invalid_lookbehind_reports_produced_size() {
        let mut output = [0; 8];
        let err = decompress(&[0x12, 0xaa, 0xc0, 0xff], &mut output).unwrap_err();

        assert_eq!(err.kind(), &ErrorKind::LookbehindOverrun);
        assert_eq!(err.dst_size(), 1);
    }

    #[test]
    fn valid_overlapping_lookbehind_is_copied() {
        // Five literals, then copy three bytes from distance one, then terminate.
        let input = [22, b'a', b'b', b'c', b'd', b'e', 0x40, 0, 17, 0, 0];
        let mut output = [0; 8];

        assert_eq!(decompress(&input, &mut output).unwrap(), 8);
        assert_eq!(&output, b"abcdeeee");
    }
}
