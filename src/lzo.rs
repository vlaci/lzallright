#![allow(non_upper_case_globals)]
use std::cmp;

use crate::error::{Error, ErrorKind};

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

    fn advance(&mut self, s: &State, match_pos: &mut usize, match_count: &mut usize, b: &[u8]) {
        let key = Self::make_key(&b[s.wind_b..]);
        let head = self.get_head(key);
        *match_pos = head as _;
        self.chain[s.wind_b] = head;
        *match_count = self.chain_sz[key] as _;
        self.chain_sz[key] += 1;
        if *match_count > MAX_MATCH_LEN {
            *match_count = MAX_MATCH_LEN
        }
        self.head[key] = s.wind_b as u16;
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

    fn search(
        &self,
        s: &State,
        lb_pos: &mut usize,
        lb_len: &mut usize,
        best_pos: &mut [usize; MaxMatchByLengthLen],
        b: &[u8],
    ) -> bool {
        let pos = self.head[Self::make_key(&b[s.wind_b..])];
        if pos == u16::MAX {
            return false;
        }
        if best_pos[2] == 0 {
            best_pos[2] = pos as usize + 1;
        }
        if *lb_len < 2 {
            *lb_len = 2;
            *lb_pos = pos as usize;
        }
        true
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

struct State {
    src_end: *const u8,
    inp: *const u8, // TODO uint8_t* maybe Cursor
    wind_sz: usize,
    wind_b: usize,
    wind_e: usize,
    cycle1_countdown: usize,
    bufp: *const u8,
    buf_sz: usize,
}

impl State {
    unsafe fn new(src: *const u8, src_size: usize, dict: &mut Dict) -> Self {
        let wind_sz = cmp::min(src_size, MAX_MATCH_LEN);
        let mut state = State {
            src_end: src.add(src_size),
            inp: src.add(wind_sz),
            wind_sz,
            wind_b: 0,
            wind_e: wind_sz,
            cycle1_countdown: MAX_DIST,
            bufp: src,
            buf_sz: 0,
        };

        let dict_buffer = dict.buffer.as_mut_ptr();
        std::ptr::copy_nonoverlapping(src, dict_buffer, wind_sz);

        if state.wind_e == BUF_SIZE {
            state.wind_e = 0;
        }

        if wind_sz < 3 {
            dict.buffer[wind_sz..wind_sz + 3].fill(0);
        }
        state
    }

    /* Access next input byte and advance both ends of circular buffer */
    unsafe fn get_byte(&mut self, buf: *mut u8) {
        if self.inp >= self.src_end {
            if self.wind_sz > 0 {
                self.wind_sz -= 1;
            }
            *buf.add(self.wind_e) = 0;
            if self.wind_e < MAX_MATCH_LEN {
                *buf.add(BUF_SIZE + self.wind_e) = 0;
            }
        } else {
            *buf.add(self.wind_e) = *self.inp;
            if self.wind_e < MAX_MATCH_LEN {
                *buf.add(BUF_SIZE + self.wind_e) = *self.inp;
            }
            self.inp = self.inp.add(1);
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

pub struct Dict {
    match3: Match3,
    match2: Match2,

    /* Circular buffer caching enough data to access the maximum lookback
     * distance of 48K + maximum match length of 2K. An additional 2K is
     * allocated so the start of the buffer may be replicated at the end,
     * therefore providing efficient circular access.
     */
    buffer: [u8; BUF_SIZE + MAX_MATCH_LEN],
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
        /* Remove match from about-to-be-clobbered buffer entry */
        if s.cycle1_countdown == 0 {
            self.match3.remove(s.wind_e, &self.buffer[..]);
            self.match2.remove(s.wind_e, &self.buffer[..]);
        } else {
            s.cycle1_countdown -= 1;
        }
    }

    unsafe fn advance(
        &mut self,
        s: &mut State,
        lb_off: &mut usize,
        lb_len: &mut usize,
        best_off: &mut [usize; MaxMatchByLengthLen],
        skip: bool,
    ) {
        if skip {
            for _ in 0..*lb_len - 1 {
                self.reset_next_input_entry(s);
                self.match3.skip_advance(s, &self.buffer[..]);
                self.match2.add(s.wind_b as u16, &self.buffer[..]);
                s.get_byte(self.buffer.as_mut_ptr());
            }
        }

        *lb_len = 1;
        *lb_off = 0;
        let mut lb_pos = 0usize;

        let mut best_pos = [0; MaxMatchByLengthLen];
        let mut match_pos = 0usize;
        let mut match_count = 0usize;

        self.match3
            .advance(s, &mut match_pos, &mut match_count, &self.buffer[..]);

        let mut best_char = Some(self.buffer[s.wind_b]);
        let best_len = *lb_len;

        if *lb_len >= s.wind_sz {
            if s.wind_sz == 0 {
                best_char = None
            }
            *lb_off = 0; // superfluous?
            self.match3.best_len[s.wind_b] = MAX_MATCH_LEN as u16 + 1;
        } else {
            if self
                .match2
                .search(s, &mut lb_pos, lb_len, &mut best_pos, &self.buffer[..])
                && s.wind_sz >= 3
            {
                for _i in 0..match_count {
                    let bufp = self.buffer.as_ptr();
                    debug_assert!(s.wind_b + s.wind_sz <= self.buffer.len());
                    debug_assert!(match_pos + s.wind_sz <= self.buffer.len());
                    let match_len = mismatch(bufp.add(s.wind_b), bufp.add(match_pos), s.wind_sz);

                    if match_len < 2 {
                        match_pos = self.match3.chain[match_pos] as usize;
                        continue;
                    }
                    if match_len < MaxMatchByLengthLen && best_pos[match_len] == 0 {
                        best_pos[match_len] = match_pos + 1;
                    }
                    if match_len > *lb_len {
                        *lb_len = match_len;
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
            if *lb_len > best_len {
                *lb_off = s.pos2off(lb_pos);
            }
            self.match3.best_len[s.wind_b] = *lb_len as u16;
            for i in 2..MaxMatchByLengthLen {
                best_off[i] = if best_pos[i] > 0 {
                    s.pos2off(best_pos[i].wrapping_sub(1))
                } else {
                    0
                };
            }
        }

        self.reset_next_input_entry(s);

        self.match2.add(s.wind_b as u16, &self.buffer[..]);

        s.get_byte(self.buffer.as_mut_ptr());

        if best_char.is_none() {
            s.buf_sz = 0;
            *lb_len = 0
            /* Signal exit */
        } else {
            s.buf_sz = s.wind_sz + 1;
        }
        s.bufp = s.inp.sub(s.buf_sz);
    }
}

#[inline(always)]
unsafe fn mismatch(a: *const u8, b: *const u8, n: usize) -> usize {
    let mut i = 0;
    while i < n {
        if *a.add(i) != *b.add(i) {
            return i;
        }
        i += 1;
    }
    n
}

fn find_better_match(
    best_off: [usize; MaxMatchByLengthLen],
    lb_len: &mut usize,
    lb_off: &mut usize,
) {
    if *lb_len <= M2MinLen || *lb_off <= M2MaxOffset {
        return;
    }
    if *lb_off > M2MaxOffset
        && *lb_len > M2MinLen
        && *lb_len <= M2MaxLen + 1
        && best_off[*lb_len - 1] != 0
        && best_off[*lb_len - 1] <= M2MaxOffset
    {
        *lb_len -= 1;
        *lb_off = best_off[*lb_len];
    } else if *lb_off > M3MaxOffset
        && *lb_len > M4MaxLen
        && *lb_len <= M2MaxLen + 2
        && best_off[*lb_len - 2] != 0
        && best_off[*lb_len] <= M2MaxOffset
    {
        *lb_len -= 2;
        *lb_off = best_off[*lb_len];
    } else if *lb_off > M3MaxOffset
        && *lb_len > M4MaxLen
        && *lb_len <= M3MaxLen + 1
        && best_off[*lb_len - 1] != 0
        && best_off[*lb_len - 2] <= M3MaxOffset
    {
        *lb_len -= 1;
        *lb_off = best_off[*lb_len];
    }
}

macro_rules! needs_out {
    ($outp:expr, $outp_end:ident, $dst:expr, $dst_size:expr, $count:expr) => {{
        if $count > $outp_end.offset_from($outp) as usize {
            *$dst_size = $outp.offset_from($dst) as usize;
            return Err(Error::new(ErrorKind::OutputOverrun, *$dst_size));
        }
    }};
}

macro_rules! write_zero_byte_length {
    ($outp:expr, $length:expr) => {{
        let mut l = $length;
        while l > 255 {
            **$outp = 0;
            *$outp = (*$outp).add(1);
            l -= 255;
        }
        **$outp = l as u8;
        *$outp = (*$outp).add(1);
    }};
}

const Max255Count: usize = !0usize / 255 - 2;

unsafe fn encode_literal_run(
    outp: *mut *mut u8,
    outp_end: *const u8,
    dst: *const u8,
    dst_size: *mut usize,
    lit_ptr: *const u8,
    lit_len: usize,
) -> Result<(), Error> {
    if (*outp).offset_from(dst) == 0 && lit_len <= 238 {
        needs_out!(*outp, outp_end, dst, dst_size, 1);
        **outp = 17 + lit_len as u8;
        *outp = (*outp).add(1);
    } else if lit_len <= 3 {
        *(*outp).sub(2) |= lit_len as u8;
    } else if lit_len <= 18 {
        needs_out!(*outp, outp_end, dst, dst_size, 1);
        **outp = lit_len as u8 - 3;
        *outp = (*outp).add(1);
    } else {
        needs_out!(*outp, outp_end, dst, dst_size, (lit_len - 18) / 255 + 2);
        **outp = 0;
        *outp = (*outp).add(1);
        write_zero_byte_length!(outp, lit_len - 18);
    }
    needs_out!(*outp, outp_end, dst, dst_size, lit_len);

    std::ptr::copy_nonoverlapping(lit_ptr, *outp, lit_len);

    //out[*outp..*outp + lit_len].copy_from_slice(lit);
    *outp = (*outp).add(lit_len);
    //*outp += lit_len;
    Ok(())
}

unsafe fn encode_lookback_match(
    outp: *mut *mut u8,
    outp_end: *const u8,
    dst: *const u8,
    dst_size: *mut usize,
    mut lb_len: usize,
    mut lb_off: usize,
    last_lit_len: usize,
) -> Result<(), Error> {
    if lb_len == 2 {
        lb_off -= 1;
        needs_out!(*outp, outp_end, dst, dst_size, 2);
        **outp = (M1Marker | ((lb_off & 0x3) << 2)) as u8;
        *outp = (*outp).add(1);
        **outp = (lb_off >> 2) as u8;
        *outp = (*outp).add(1);
    } else if lb_len <= M2MaxLen && lb_off <= M2MaxOffset {
        lb_off -= 1;
        needs_out!(*outp, outp_end, dst, dst_size, 2);
        **outp = ((lb_len - 1) << 5 | ((lb_off & 0x7) << 2)) as u8;
        *outp = (*outp).add(1);
        **outp = (lb_off >> 3) as u8;
        *outp = (*outp).add(1);
    } else if lb_len == M2MinLen && lb_off <= M1MaxOffset + M2MaxOffset && last_lit_len >= 4 {
        lb_off -= 1 + M2MaxOffset;
        needs_out!(*outp, outp_end, dst, dst_size, 2);
        **outp = (M1Marker | ((lb_off & 0x3) << 2)) as u8;
        *outp = (*outp).add(1);
        **outp = (lb_off >> 2) as u8;
        *outp = (*outp).add(1);
    } else if lb_off <= M3MaxOffset {
        lb_off -= 1;
        if lb_len <= M3MaxLen {
            needs_out!(*outp, outp_end, dst, dst_size, 1);
            **outp = (M3Marker | (lb_len - 2)) as u8;
            *outp = (*outp).add(1);
        } else {
            lb_len -= M3MaxLen;
            needs_out!(*outp, outp_end, dst, dst_size, lb_len / 255 + 2);
            **outp = M3Marker as u8;
            *outp = (*outp).add(1);
            write_zero_byte_length!(outp, lb_len);
        }
        needs_out!(*outp, outp_end, dst, dst_size, 2);
        **outp = (lb_off << 2) as u8;
        *outp = (*outp).add(1);
        **outp = (lb_off >> 6) as u8;
        *outp = (*outp).add(1);
    } else {
        lb_off -= 0x4000;
        if lb_len <= M4MaxLen {
            needs_out!(*outp, outp_end, dst, dst_size, 1);
            **outp = (M4Marker | ((lb_off & 0x4000) >> 11) | (lb_len - 2)) as u8;
            *outp = (*outp).add(1);
        } else {
            lb_len -= M4MaxLen;
            needs_out!(*outp, outp_end, dst, dst_size, lb_len / 255 + 2);
            **outp = (M4Marker | ((lb_off & 0x4000) >> 11)) as u8;
            *outp = (*outp).add(1);
            write_zero_byte_length!(outp, lb_len);
        }
        needs_out!(*outp, outp_end, dst, dst_size, 2);
        **outp = (lb_off << 2) as u8;
        *outp = (*outp).add(1);
        **outp = (lb_off >> 6) as u8;
        *outp = (*outp).add(1);
    }

    Ok(())
}

pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, Error> {
    unsafe { decompress_inner(src.as_ptr(), src.len(), dst.as_mut_ptr(), dst.len()) }
}

unsafe fn decompress_inner(
    src: *const u8,
    src_size: usize,
    dst: *mut u8,
    init_dst_size: usize,
) -> Result<usize, Error> {
    let mut dst_size = init_dst_size;

    if src_size < 3 {
        return Err(Error::new(ErrorKind::InputOverrun, 0));
    }

    let mut inp = src;
    let inp_end = src.add(src_size);
    let mut outp = dst;
    let outp_end = dst.add(dst_size);
    let mut lbcur;
    let mut lbdist;
    let mut lblen;
    let mut state = 0;
    let mut nstate;

    macro_rules! needs_in {
        ($count:expr) => {
            if $count > inp_end.offset_from(inp) as usize {
                dst_size = outp.offset_from(dst) as usize;
                return Err(Error::new(ErrorKind::InputOverrun, dst_size));
            }
        };
    }

    macro_rules! needs_out {
        ($count:expr) => {{
            if $count > outp_end.offset_from(outp) as usize {
                dst_size = outp.offset_from(dst) as usize;
                return Err(Error::new(ErrorKind::OutputOverrun, dst_size));
            }
        }};
    }

    macro_rules! consume_zero_byte_length {
        () => {{
            let mut offset = 0;
            while (inp < inp_end && *inp == 0) {
                // fuzzfix
                needs_in!(1);
                inp = inp.add(1);
                offset += 1;
            }
            if offset > Max255Count {
                return Err(Error::new(ErrorKind::Error, outp.offset_from(dst) as usize));
            }
            offset
        }};
    }

    /* First byte encoding */
    if *inp >= 22 {
        /* 22..255 : copy literal string
         *           length = (byte - 17) = 4..238
         *           state = 4 [ don't copy extra literals ]
         *           skip byte
         */
        let len = (*inp - 17) as usize;
        inp = inp.add(1);
        needs_in!(len);
        needs_out!(len);
        std::ptr::copy_nonoverlapping(inp, outp, len);
        inp = inp.add(len);
        outp = outp.add(len);
        state = 4;
    } else if *inp >= 18 {
        /* 18..21 : copy 0..3 literals
         *          state = (byte - 17) = 0..3  [ copy <state> literals ]
         *          skip byte
         */
        nstate = (*inp - 17) as usize;
        inp = inp.add(1);
        state = nstate;
        needs_in!(nstate);
        needs_out!(nstate);
        std::ptr::copy_nonoverlapping(inp, outp, nstate);
        inp = inp.add(nstate);
        outp = outp.add(nstate);
    }
    /* 0..17 : follow regular instruction encoding, see below. It is worth
     *         noting that codes 16 and 17 will represent a block copy from
     *         the dictionary which is empty, and that they will always be
     *         invalid at this place.
     */

    loop {
        needs_in!(1);
        let inst = *inp as usize;
        inp = inp.add(1);
        if inst & 0xC0 != 0 {
            /* [M2]
             * 1 L L D D D S S  (128..255)
             *   Copy 5-8 bytes from block within 2kB distance
             *   state = S (copy S literals after this block)
             *   length = 5 + L
             * Always followed by exactly one byte : H H H H H H H H
             *   distance = (H << 3) + D + 1
             *
             * 0 1 L D D D S S  (64..127)
             *   Copy 3-4 bytes from block within 2kB distance
             *   state = S (copy S literals after this block)
             *   length = 3 + L
             * Always followed by exactly one byte : H H H H H H H H
             *   distance = (H << 3) + D + 1
             */
            needs_in!(1);
            lbdist = ((*inp as usize) << 3) + ((inst >> 2) & 0x7) + 1;
            inp = inp.add(1);
            lblen = (inst >> 5) + 1;
            nstate = inst & 0x3;
        } else if inst & M3Marker != 0 {
            /* [M3]
             * 0 0 1 L L L L L  (32..63)
             *   Copy of small block within 16kB distance (preferably less than 34B)
             *   length = 2 + (L ?: 31 + (zero_bytes * 255) + non_zero_byte)
             * Always followed by exactly one LE16 :  D D D D D D D D : D D D D D D S S
             *   distance = D + 1
             *   state = S (copy S literals after this block)
             */
            lblen = (inst & 0x1f) + 2;
            if lblen == 2 {
                let offset = consume_zero_byte_length!();
                needs_in!(1);
                lblen += offset * 255 + 31 + *inp as usize;
                inp = inp.add(1);
            }
            needs_in!(2);
            nstate = get_le16(inp);
            inp = inp.add(2);
            lbdist = (nstate >> 2) + 1;
            nstate &= 0x3;
        } else if inst & M4Marker != 0 {
            /* [M4]
             * 0 0 0 1 H L L L  (16..31)
             *   Copy of a block within 16..48kB distance (preferably less than 10B)
             *   length = 2 + (L ?: 7 + (zero_bytes * 255) + non_zero_byte)
             * Always followed by exactly one LE16 :  D D D D D D D D : D D D D D D S S
             *   distance = 16384 + (H << 14) + D
             *   state = S (copy S literals after this block)
             *   End of stream is reached if distance == 16384
             */
            lblen = (inst & 0x7) + 2;
            if lblen == 2 {
                let offset = consume_zero_byte_length!();
                needs_in!(1);
                lblen += offset * 255 + 7 + *inp as usize;
                inp = inp.add(1);
            }
            needs_in!(2);
            nstate = get_le16(inp);
            inp = inp.add(2);
            lbdist = ((inst & 0x8) << 11) + (nstate >> 2);
            nstate &= 0x3;
            if lbdist == 0 {
                break; /* Stream finished */
            }
            lbdist += 16384;
        } else {
            /* [M1] Depends on the number of literals copied by the last instruction. */
            if state == 0 {
                /* If last instruction did not copy any literal (state == 0), this
                 * encoding will be a copy of 4 or more literal, and must be interpreted
                 * like this :
                 *
                 *    0 0 0 0 L L L L  (0..15)  : copy long literal string
                 *    length = 3 + (L ?: 15 + (zero_bytes * 255) + non_zero_byte)
                 *    state = 4  (no extra literals are copied)
                 */
                let mut len = inst + 3;
                if len == 3 {
                    let offset = consume_zero_byte_length!();
                    needs_in!(1);
                    len += offset * 255 + 15 + *inp as usize;
                    inp = inp.add(1);
                }
                /* copy_literal_run */
                needs_in!(len);
                needs_out!(len);
                std::ptr::copy_nonoverlapping(inp, outp, len);
                outp = outp.add(len);
                inp = inp.add(len);
                state = 4;
                continue;
            } else if state != 4 {
                /* If last instruction used to copy between 1 to 3 literals (encoded in
                 * the instruction's opcode or distance), the instruction is a copy of a
                 * 2-byte block from the dictionary within a 1kB distance. It is worth
                 * noting that this instruction provides little savings since it uses 2
                 * bytes to encode a copy of 2 other bytes but it encodes the number of
                 * following literals for free. It must be interpreted like this :
                 *
                 *    0 0 0 0 D D S S  (0..15)  : copy 2 bytes from <= 1kB distance
                 *    length = 2
                 *    state = S (copy S literals after this block)
                 *  Always followed by exactly one byte : H H H H H H H H
                 *    distance = (H << 2) + D + 1
                 */
                needs_in!(1);
                nstate = inst & 0x3;
                lbdist = (inst >> 2) + ((*inp as usize) << 2) + 1;
                inp = inp.add(1);
                lblen = 2;
            } else {
                /* If last instruction used to copy 4 or more literals (as detected by
                 * state == 4), the instruction becomes a copy of a 3-byte block from the
                 * dictionary from a 2..3kB distance, and must be interpreted like this :
                 *
                 *    0 0 0 0 D D S S  (0..15)  : copy 3 bytes from 2..3 kB distance
                 *    length = 3
                 *    state = S (copy S literals after this block)
                 *  Always followed by exactly one byte : H H H H H H H H
                 *    distance = (H << 2) + D + 2049
                 */
                needs_in!(1);
                nstate = inst & 0x3;
                lbdist = (inst >> 2) + ((*inp as usize) << 2) + 2049;
                inp = inp.add(1);
                lblen = 3;
            }
        }
        if lbdist > outp.offset_from(dst) as usize {
            let dst_size = outp.offset_from(dst) as usize;
            return Err(Error::new(ErrorKind::LookbehindOverrun, dst_size));
        }
        lbcur = outp.wrapping_sub(lbdist);

        needs_in!(nstate);
        needs_out!(lblen + nstate);
        /* Copy lookbehind */
        // NOTE: cannot use `copy_within`, as the algorithm depends on the
        // quirky behavior of overlap handling
        // dst.copy_within(lbcur..lbcur + lblen, outp);
        for _ in 0..lblen {
            *outp = *lbcur;
            outp = outp.add(1);
            lbcur = lbcur.add(1);
        }
        state = nstate;
        /* Copy literal */
        // std::ptr::copy_nonoverlapping(inp, outp, nstate);
        for _ in 0..nstate {
            *outp = *inp;
            inp = inp.add(1);
            outp = outp.add(1);
        }
    }

    let dst_size = outp.offset_from(dst) as usize;
    if lblen != 3 {
        /* Ensure terminating M4 was encountered */
        return Err(Error::new(ErrorKind::Error, dst_size));
    }
    if inp == inp_end {
        Ok(dst_size)
    } else if inp < inp_end {
        Err(Error::new(ErrorKind::InputNotConsumed, dst_size))
    } else {
        Err(Error::new(ErrorKind::InputOverrun, dst_size))
    }
}

/// # Safety
///
/// `p` must point to at least 2 readable bytes.
#[inline(always)]
unsafe fn get_le16(p: *const u8) -> usize {
    let lsb = *p;
    let msb = *p.add(1);
    ((msb as usize) << 8) | lsb as usize
}

pub fn compress(src: &[u8], out: &mut [u8], dict: &mut Dict) -> Result<usize, Error> {
    unsafe { compress_internal(src.as_ptr(), src.len(), out.as_mut_ptr(), out.len(), dict) }
}

unsafe fn compress_internal(
    src: *const u8,
    src_size: usize,
    dst: *mut u8,
    mut dst_size: usize,
    dict: &mut Dict,
) -> Result<usize, Error> {
    dict.reset();
    let mut s = State::new(src, src_size, dict);
    let mut outp = dst;
    let outp_end = outp.add(dst_size);
    let mut lit_len = 0;
    let mut lit_ptr = s.inp;
    let mut lb_len = 0;
    let mut lb_off = 0;
    let mut best_off = [0; MaxMatchByLengthLen];

    dict.advance(&mut s, &mut lb_off, &mut lb_len, &mut best_off, false);

    while s.buf_sz > 0 {
        if lit_len == 0 {
            lit_ptr = s.bufp;
        }
        #[allow(clippy::if_same_then_else)]
        if lb_len < 2
            || (lb_len == 2 && (lb_off > M1MaxOffset || lit_len == 0 || lit_len >= 4))
            || (lb_len == 2 && outp == dst)
            || (outp == dst && lit_len == 0)
        {
            lb_len = 0;
        } else if lb_len == M2MinLen && lb_off > M1MaxOffset + M2MaxOffset && lit_len >= 4 {
            lb_len = 0;
        }
        if lb_len == 0 {
            lit_len += 1;
            dict.advance(&mut s, &mut lb_off, &mut lb_len, &mut best_off, false);
            continue;
        }
        find_better_match(best_off, &mut lb_len, &mut lb_off);
        encode_literal_run(&mut outp, outp_end, dst, &mut dst_size, lit_ptr, lit_len)?;
        encode_lookback_match(
            &mut outp,
            outp_end,
            dst,
            &mut dst_size,
            lb_len,
            lb_off,
            lit_len,
        )?;
        lit_len = 0;
        dict.advance(&mut s, &mut lb_off, &mut lb_len, &mut best_off, true);
    }
    encode_literal_run(&mut outp, outp_end, dst, &mut dst_size, lit_ptr, lit_len)?;

    /* Terminating M4 */
    needs_out!(outp, outp_end, dst, &mut dst_size, 3);
    *outp = (M4Marker | 1) as u8;
    outp = outp.add(1);
    *outp = 0;
    outp = outp.add(1);
    *outp = 0;
    outp = outp.add(1);

    dst_size = outp.offset_from(dst) as usize;
    Ok(dst_size)
}

#[cfg(test)]
mod tests {
    use super::*;

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
