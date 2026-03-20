#![allow(non_upper_case_globals)]
use std::cmp;
use std::io::Write;

#[derive(Debug, PartialEq)]
pub(crate) enum EResult {
    LookbehindOverrun,
    OutputOverrun,
    InputOverrun,
    Error,
    InputNotConsumed(usize),
}

const HASH_SIZE: usize = 0x4000;
const MAX_DIST: usize = 0xbfff;
const MAX_MATCH_LEN: usize = 0x800;
const BUF_SIZE: usize = MAX_DIST + MAX_MATCH_LEN;

const M1MaxOffset: usize = 0x0400;
const M2MaxOffset: usize = 0x0800;
const M3MaxOffset: usize = 0x4000;
const M4MaxOffset: usize = 0xbfff;

const M1MinLen: usize = 2;
const M1MaxLen: usize = 2;
const M2MinLen: usize = 3;
const M2MaxLen: usize = 8;
const M3MinLen: usize = 3;
const M3MaxLen: usize = 33;
const M4MinLen: usize = 3;
const M4MaxLen: usize = 9;

const M1Marker: usize = 0x0;
const M2Marker: usize = 0x40;
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
        // (((0x9f5fusize.wrapping_mul((((data[0] as u32) << 5 ^ data[1] as u32) << 5) ^ data[2] as u32))
        //     >> 5)
        //     & 0x3fff) as usize
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
        self.chain_sz[Self::make_key(&b[pos..])] -= 1;
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
        self.chain_sz[key] += 1
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

struct State<'a> {
    src: &'a [u8],
    inp: usize, // TODO uint8_t* maybe Cursor
    wind_sz: usize,
    wind_b: usize,
    wind_e: usize,
    cycle1_countdown: usize,
    bufp: usize, // TODO: uint8_t*
    buf_sz: usize,
}

impl<'a> State<'a> {
    fn new(src: &'a [u8], dict: &mut Dict) -> Self {
        let wind_sz = cmp::min(src.len(), MAX_MATCH_LEN);
        let state = State {
            src,
            inp: wind_sz,
            wind_sz,
            wind_b: 0,
            wind_e: wind_sz,
            cycle1_countdown: MAX_DIST,
            bufp: 0,
            buf_sz: 0,
        };

        dict.buffer[..wind_sz].copy_from_slice(&src[..wind_sz]);
        if wind_sz < 3 {
            dict.buffer[wind_sz..wind_sz + 3].fill(0);
        }
        state
    }

    /* Access next input byte and advance both ends of circular buffer */
    fn get_byte(&mut self, buf: &mut [u8]) {
        if self.inp >= self.src.len() {
            if self.wind_sz > 0 {
                self.wind_sz -= 1;
            }
            buf[self.wind_e] = 0;
            if self.wind_e < MAX_MATCH_LEN {
                buf[BUF_SIZE + self.wind_e] = 0;
            }
        } else {
            buf[self.wind_e] = self.src[self.inp];
            if self.wind_e < MAX_MATCH_LEN {
                buf[BUF_SIZE + self.wind_e] = self.src[self.inp];
            }
            self.inp += 1;
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

pub(crate) struct Dict {
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
    pub(crate) fn new() -> Self {
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

    fn advance(
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
                s.get_byte(&mut self.buffer[..]);
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
                    let match_len = mismatch(
                        &self.buffer[s.wind_b..s.wind_b + s.wind_sz],
                        &self.buffer[match_pos..],
                    );

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

        s.get_byte(&mut self.buffer[..]);

        if best_char.is_none() {
            s.buf_sz = 0;
            *lb_len = 0
            /* Signal exit */
        } else {
            s.buf_sz = s.wind_sz + 1;
        }
        s.bufp = s.inp - s.buf_sz;
    }
}

fn mismatch(a: &[u8], b: &[u8]) -> usize {
    let min_len = std::cmp::min(a.len(), b.len());
    for i in 0..min_len {
        if a[i] != b[i] {
            return i;
        }
    }
    min_len
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
    ($out:ident, $count:expr) => {{
        if $out.len() < $count {
            return Err(EResult::OutputOverrun);
        }
    }};
}

macro_rules! needs_in {
    ($out:ident, $count:expr) => {{
        if $out.len() < $count {
            return Err(EResult::InputOverrun);
        }
    }};
}

macro_rules! write_zero_byte_length {
    ($out:ident, $pos:ident, $length:expr) => {{
        let mut l = $length;
        while l > 255 {
            $out[*$pos] = 0;
            *$pos += 1;
            l -= 255;
        }
        $out[*$pos] = l as u8;
        *$pos += 1;
    }};
}

const Max255Count: usize = !0usize / 255 - 2;

fn consume_zero_byte_length(src: &[u8]) -> Result<usize, EResult> {
    let mut offset = 0;
    while src[offset] == 0 {
        offset += 1;
        if offset > Max255Count {
            return Err(EResult::Error);
        }
    }
    Ok(offset)
}

fn encode_literal_run(out: &mut [u8], outp: &mut usize, lit: &[u8]) -> Result<(), EResult> {
    if *outp == 0 && lit.len() <= 238 {
        needs_out!(out, 1);
        out[*outp] = 17 + lit.len() as u8;
        *outp += 1;
    } else if lit.len() <= 3 {
        out[*outp - 2] |= lit.len() as u8;
    } else if lit.len() <= 18 {
        needs_out!(out, 1);
        out[*outp] = lit.len() as u8 - 3;
        *outp += 1;
    } else {
        needs_out!(out, (lit.len() - 18) / 255 + 2);
        out[*outp] = 0;
        *outp += 1;
        write_zero_byte_length!(out, outp, lit.len() - 18);
    }
    needs_out!(out, lit.len());

    out[*outp..*outp + lit.len()].copy_from_slice(lit);
    *outp += lit.len();
    Ok(())
}

fn encode_lookback_match(
    out: &mut [u8],
    outp: &mut usize,
    mut lb_len: usize,
    mut lb_off: usize,
    last_lit_len: usize,
) -> Result<(), EResult> {
    if lb_len == 2 {
        lb_off -= 1;
        needs_out!(out, 2);
        out[*outp] = (M1Marker | ((lb_off & 0x3) << 2)) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 2) as u8;
        *outp += 1;
    } else if lb_len <= M2MaxLen && lb_off <= M2MaxOffset {
        lb_off -= 1;
        needs_out!(out, 2);
        out[*outp] = ((lb_len - 1) << 5 | ((lb_off & 0x7) << 2)) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 3) as u8;
        *outp += 1;
    } else if lb_len == M2MinLen && lb_off <= M1MaxOffset + M2MaxOffset && last_lit_len >= 4 {
        lb_off -= 1 + M2MaxOffset;
        needs_out!(out, 2);
        out[*outp] = (M1Marker | ((lb_off & 0x3) << 2)) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 2) as u8;
        *outp += 1;
    } else if lb_off <= M3MaxOffset {
        lb_off -= 1;
        if lb_len <= M3MaxLen {
            needs_out!(out, 1);
            out[*outp] = (M3Marker | (lb_len - 2)) as u8;
            *outp += 1;
        } else {
            lb_len -= M3MaxLen;
            needs_out!(out, lb_len / 255 + 2);
            out[*outp] = M3Marker as u8;
            *outp += 1;
            write_zero_byte_length!(out, outp, lb_len);
        }
        needs_out!(out, 2);
        out[*outp] = (lb_off << 2) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 6) as u8;
        *outp += 1;
    } else {
        lb_off -= 0x4000;
        if lb_len <= M4MaxLen {
            needs_out!(out, 1);
            out[*outp] = (M4Marker | ((lb_off & 0x4000) >> 11) | (lb_len - 2)) as u8;
            *outp += 1;
        } else {
            lb_len -= M4MaxLen;
            needs_out!(out, lb_len / 255 + 2);
            out[*outp] = (M4Marker | ((lb_off & 0x4000) >> 11)) as u8;
            *outp += 1;
            write_zero_byte_length!(out, outp, lb_len);
        }
        needs_out!(out, 2);
        out[*outp] = (lb_off << 2) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 6) as u8;
        *outp += 1;
    }

    Ok(())
}

pub(crate) fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, EResult> {
    if src.len() < 3 {
        return Err(EResult::InputOverrun);
    }

    let mut inp = 0;
    let mut outp = 0;
    let mut lbcur = 0;
    let mut lblen = 0;
    let mut state = 0;
    let mut nstate = 0;

    /* First byte encoding */
    if src[inp] >= 22 {
        /* 22..255 : copy literal string
         *           length = (byte - 17) = 4..238
         *           state = 4 [ don't copy extra literals ]
         *           skip byte
         */
        let len = (src[inp] - 17) as usize;
        inp += 1;
        needs_in!(src, len);
        needs_out!(dst, len);
        dst[outp..outp + len].copy_from_slice(&src[inp..inp + len]);
        inp += len;
        outp += len;
        state = 4;
    } else if src[inp] >= 18 {
        /* 18..21 : copy 0..3 literals
         *          state = (byte - 17) = 0..3  [ copy <state> literals ]
         *          skip byte
         */
        nstate = (src[inp] - 17) as usize;
        inp += 1;
        state = nstate;
        needs_in!(src, nstate);
        needs_out!(dst, nstate);
        dst[outp..outp + nstate].copy_from_slice(&src[inp..inp + nstate]);
        inp += nstate;
        outp += nstate;
    }
    /* 0..17 : follow regular instruction encoding, see below. It is worth
     *         noting that codes 16 and 17 will represent a block copy from
     *         the dictionary which is empty, and that they will always be
     *         invalid at this place.
     */

    loop {
        needs_in!(src, 1);
        let inst = src[inp] as usize;
        inp += 1;
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
            needs_in!(src, 1);
            lbcur = ((src[inp] as usize) << 3) + ((inst >> 2) & 0x7) + 1;
            inp += 1;
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
                let offset = consume_zero_byte_length(&src[inp..])?;
                inp += offset;
                needs_in!(src, 1);
                lblen += offset * 255 + 31 + src[inp] as usize;
                inp += 1;
            }
            needs_in!(src, 2);
            nstate = get_le16(&src[inp..]);
            inp += 2;
            lbcur = (nstate >> 2) + 1;
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
                let offset = consume_zero_byte_length(&src[inp..])?;
                inp += offset;
                needs_in!(src, 1);
                lblen += offset * 255 + 7 + src[inp] as usize;
                inp += 1;
            }
            needs_in!(src, 2);
            nstate = get_le16(&src[inp..]);
            inp += 2;
            lbcur = ((inst & 0x8) << 11) + (nstate >> 2);
            nstate &= 0x3;
            if lbcur == 0 {
                break; /* Stream finished */
            }
            lbcur += 16384;
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
                    let offset = consume_zero_byte_length(&src[inp..])?;
                    inp += offset;
                    needs_in!(src, 1);
                    len += offset * 255 + 15 + src[inp] as usize;
                    inp += 1;
                }
                /* copy_literal_run */
                needs_in!(src, len);
                needs_out!(dst, len);
                dst[outp..outp + len].copy_from_slice(&src[inp..inp + len]);
                outp += len;
                inp += len;
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
                needs_in!(src, 1);
                nstate = inst & 0x3;
                lbcur = (inst >> 2) + ((src[inp] as usize) << 2) + 1;
                inp += 1;
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
                needs_in!(src, 1);
                nstate = inst & 0x3;
                lbcur = (inst >> 2) + (src[inp] << 2) as usize + 2049;
                inp += 1;
                lblen = 3;
            }
        }
        if lbcur > outp {
            let dst_size = outp - outp;
            return Err(EResult::LookbehindOverrun);
        }
        lbcur = outp - lbcur;

        needs_in!(src, nstate);
        needs_out!(dst, lblen + nstate);
        /* Copy lookbehind */
        // NOTE: cannot use `copy_within`, as the algorithm depends on the
        // quirky behavior of overlap handling
        // dst.copy_within(lbcur..lbcur + lblen, outp);
        for i in 0..lblen {
            dst[outp + i] = dst[lbcur + i];
        }

        //dst.copy_within(lbcur..(lbcur + lblen), outp);
        outp += lblen;
        lbcur += lblen;
        state = nstate;
        /* Copy literal */
        dst[outp..outp + nstate].copy_from_slice(&src[inp..inp + nstate]);
        inp += nstate;
        outp += nstate;
    }

    let dst_size = outp - outp;
    if lblen != 3 {
        /* Ensure terminating M4 was encountered */
        return Err(EResult::Error);
    }
    if inp == src.len() {
        Ok(outp)
    } else if inp < src.len() {
        Err(EResult::InputNotConsumed(outp))
    } else {
        Err(EResult::InputOverrun)
    }
}

fn get_le16(buff: &[u8]) -> usize {
    let lsb = buff[0];
    let msb = buff[1];
    ((msb as usize) << 8) | lsb as usize
}

fn read_zero_byte_length(src: &[u8], inp: &mut usize) -> Result<usize, EResult> {
    let mut length = 0;
    while *inp < src.len() && src[*inp] == 0 {
        length += 255;
        *inp += 1;
    }
    if *inp >= src.len() {
        return Err(EResult::InputOverrun);
    }
    length += src[*inp] as usize;
    *inp += 1;
    Ok(length)
}

fn copy_from_lookbehind(
    dst: &mut [u8],
    outp: usize,
    offset: usize,
    len: usize,
) -> Result<(), EResult> {
    if offset > outp {
        return Err(EResult::LookbehindOverrun);
    }

    let start = outp - offset;
    for i in 0..len {
        if start + i >= outp {
            return Err(EResult::LookbehindOverrun);
        }
        if outp + i >= dst.len() {
            return Err(EResult::OutputOverrun);
        }
        dst[outp + i] = dst[start + i];
    }
    Ok(())
}

pub(crate) fn compress(src: &[u8], out: &mut [u8], dict: &mut Dict) -> Result<usize, EResult> {
    dict.reset();
    let mut s = State::new(src, dict);
    let mut outp = 0; // outp == dst in cpp means outp == 0 in rust
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
        if lb_len < 2
            || (lb_len == 2 && (lb_off > M1MaxOffset || lit_len == 0 || lit_len >= 4))
            || (lb_len == 2 && outp == 0)
            || (outp == 0 && lit_len == 0)
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
        encode_literal_run(out, &mut outp, &src[lit_ptr..lit_ptr + lit_len])?;
        encode_lookback_match(out, &mut outp, lb_len, lb_off, lit_len)?;
        lit_len = 0;
        dict.advance(&mut s, &mut lb_off, &mut lb_len, &mut best_off, true);
    }
    encode_literal_run(out, &mut outp, &src[lit_ptr..lit_ptr + lit_len])?;

    /* Terminating M4 */
    needs_out!(out, 3);
    out[outp] = (M4Marker | 1) as u8;
    outp += 1;
    out[outp] = 0;
    outp += 1;
    out[outp] = 0;
    outp += 1;

    Ok(outp)
}

const MAX_255_COUNT: usize = usize::MAX / 255 - 2;
