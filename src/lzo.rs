use std::cmp;

#[derive(Debug, PartialEq)]
pub enum EResult {
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

const M1_MAX_OFFSET: usize = 0x0400;
const M2_MAX_OFFSET: usize = 0x0800;
const M3_MAX_OFFSET: usize = 0x4000;
const M2_MIN_LEN: usize = 3;
const M2_MAX_LEN: usize = 8;
const M3_MAX_LEN: usize = 33;
const M4_MAX_LEN: usize = 9;

const M1_MARKER: usize = 0x0;
const M3_MARKER: usize = 0x20;
const M4_MARKER: usize = 0x10;

const MAX_MATCH_BY_LENGTH_LEN: usize = 34; /* Max M3 len + 1 */

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
        best_pos: &mut [usize; MAX_MATCH_BY_LENGTH_LEN],
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
    pub fn new() -> Box<Self> {
        Box::default()
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
        best_off: &mut [usize; MAX_MATCH_BY_LENGTH_LEN],
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

        let mut best_pos = [0; MAX_MATCH_BY_LENGTH_LEN];
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
                    if match_len < MAX_MATCH_BY_LENGTH_LEN && best_pos[match_len] == 0 {
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
            for i in 2..MAX_MATCH_BY_LENGTH_LEN {
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
    best_off: [usize; MAX_MATCH_BY_LENGTH_LEN],
    lb_len: &mut usize,
    lb_off: &mut usize,
) {
    if *lb_len <= M2_MIN_LEN || *lb_off <= M2_MAX_OFFSET {
        return;
    }
    if *lb_off > M2_MAX_OFFSET
        && *lb_len > M2_MIN_LEN
        && *lb_len <= M2_MAX_LEN + 1
        && best_off[*lb_len - 1] != 0
        && best_off[*lb_len - 1] <= M2_MAX_OFFSET
    {
        *lb_len -= 1;
        *lb_off = best_off[*lb_len];
    } else if *lb_off > M3_MAX_OFFSET
        && *lb_len > M4_MAX_LEN
        && *lb_len <= M2_MAX_LEN + 2
        && best_off[*lb_len - 2] != 0
        && best_off[*lb_len] <= M2_MAX_OFFSET
    {
        *lb_len -= 2;
        *lb_off = best_off[*lb_len];
    } else if *lb_off > M3_MAX_OFFSET
        && *lb_len > M4_MAX_LEN
        && *lb_len <= M3_MAX_LEN + 1
        && best_off[*lb_len - 1] != 0
        && best_off[*lb_len - 2] <= M3_MAX_OFFSET
    {
        *lb_len -= 1;
        *lb_off = best_off[*lb_len];
    }
}

#[inline(always)]
fn needs_out(dst: &[u8], pos: usize, count: usize) -> Result<(), EResult> {
    if dst.len() - pos < count {
        Err(EResult::OutputOverrun)
    } else {
        Ok(())
    }
}

#[inline(always)]
fn write_zero_byte_length(out: &mut [u8], pos: &mut usize, length: usize) {
    let mut l = length;
    while l > 255 {
        out[*pos] = 0;
        *pos += 1;
        l -= 255;
    }
    out[*pos] = l as u8;
    *pos += 1;
}

const MAX_255_COUNT: usize = !0usize / 255 - 2;

fn encode_literal_run(out: &mut [u8], outp: &mut usize, lit: &[u8]) -> Result<(), EResult> {
    if *outp == 0 && lit.len() <= 238 {
        needs_out(out, *outp, 1)?;
        out[*outp] = 17 + lit.len() as u8;
        *outp += 1;
    } else if lit.len() <= 3 {
        out[*outp - 2] |= lit.len() as u8;
    } else if lit.len() <= 18 {
        needs_out(out, *outp, 1)?;
        out[*outp] = lit.len() as u8 - 3;
        *outp += 1;
    } else {
        needs_out(out, *outp, (lit.len() - 18) / 255 + 2)?;
        out[*outp] = 0;
        *outp += 1;
        write_zero_byte_length(out, outp, lit.len() - 18);
    }
    needs_out(out, *outp, lit.len())?;

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
        needs_out(out, *outp, 2)?;
        out[*outp] = (M1_MARKER | ((lb_off & 0x3) << 2)) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 2) as u8;
        *outp += 1;
    } else if lb_len <= M2_MAX_LEN && lb_off <= M2_MAX_OFFSET {
        lb_off -= 1;
        needs_out(out, *outp, 2)?;
        out[*outp] = ((lb_len - 1) << 5 | ((lb_off & 0x7) << 2)) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 3) as u8;
        *outp += 1;
    } else if lb_len == M2_MIN_LEN && lb_off <= M1_MAX_OFFSET + M2_MAX_OFFSET && last_lit_len >= 4 {
        lb_off -= 1 + M2_MAX_OFFSET;
        needs_out(out, *outp, 2)?;
        out[*outp] = (M1_MARKER | ((lb_off & 0x3) << 2)) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 2) as u8;
        *outp += 1;
    } else if lb_off <= M3_MAX_OFFSET {
        lb_off -= 1;
        if lb_len <= M3_MAX_LEN {
            needs_out(out, *outp, 1)?;
            out[*outp] = (M3_MARKER | (lb_len - 2)) as u8;
            *outp += 1;
        } else {
            lb_len -= M3_MAX_LEN;
            needs_out(out, *outp, lb_len / 255 + 2)?;
            out[*outp] = M3_MARKER as u8;
            *outp += 1;
            write_zero_byte_length(out, outp, lb_len);
        }
        needs_out(out, *outp, 2)?;
        out[*outp] = (lb_off << 2) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 6) as u8;
        *outp += 1;
    } else {
        lb_off -= 0x4000;
        if lb_len <= M4_MAX_LEN {
            needs_out(out, *outp, 1)?;
            out[*outp] = (M4_MARKER | ((lb_off & 0x4000) >> 11) | (lb_len - 2)) as u8;
            *outp += 1;
        } else {
            lb_len -= M4_MAX_LEN;
            needs_out(out, *outp, lb_len / 255 + 2)?;
            out[*outp] = (M4_MARKER | ((lb_off & 0x4000) >> 11)) as u8;
            *outp += 1;
            write_zero_byte_length(out, outp, lb_len);
        }
        needs_out(out, *outp, 2)?;
        out[*outp] = (lb_off << 2) as u8;
        *outp += 1;
        out[*outp] = (lb_off >> 6) as u8;
        *outp += 1;
    }

    Ok(())
}

/// Decompress LZO-compressed data.
///
/// # Safety
///
/// Uses raw pointer arithmetic internally for performance parity with C++.
/// All pointer accesses are guarded by `NEEDS_IN`/`NEEDS_OUT` bounds checks
/// that validate against `inp_end`/`outp_end` before any dereference.
pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, EResult> {
    if src.len() < 3 {
        return Err(EResult::InputOverrun);
    }

    unsafe { decompress_inner(src.as_ptr(), src.len(), dst.as_mut_ptr(), dst.len()) }
}

/// Raw pointer implementation matching C++ lzokay structure.
///
/// # Safety
///
/// - `src_base` must point to `src_len` readable bytes
/// - `dst_base` must point to `dst_len` writable bytes
unsafe fn decompress_inner(
    src_base: *const u8,
    src_len: usize,
    dst_base: *mut u8,
    dst_len: usize,
) -> Result<usize, EResult> {
    let mut inp = src_base;
    let inp_end = src_base.add(src_len);
    let mut outp = dst_base;
    let outp_end = dst_base.add(dst_len);

    let mut lbcur: *const u8;
    let mut lblen;
    let mut state: usize = 0;
    let mut nstate;

    macro_rules! needs_in {
        ($count:expr) => {
            if (inp_end as usize).wrapping_sub(inp as usize) < $count {
                return Err(EResult::InputOverrun);
            }
        };
    }
    macro_rules! needs_out {
        ($count:expr) => {
            if (outp_end as usize).wrapping_sub(outp as usize) < $count {
                return Err(EResult::OutputOverrun);
            }
        };
    }
    // Compute lookbehind pointer, returning LookbehindOverrun if the
    // offset would land before the start of the output buffer.
    macro_rules! lookbehind {
        ($off:expr) => {{
            let off = $off;
            if off > (outp as usize - dst_base as usize) {
                return Err(EResult::LookbehindOverrun);
            }
            outp.sub(off)
        }};
    }
    // Scans forward from `inp` counting consecutive zero bytes for
    // variable-length encoding. Returns the offset of the first non-zero byte.
    macro_rules! consume_zero_byte_length {
        () => {{
            let mut offset = 0usize;
            loop {
                needs_in!(offset + 1);
                if *inp.add(offset) != 0 {
                    break;
                }
                offset += 1;
                if offset > MAX_255_COUNT {
                    return Err(EResult::Error);
                }
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
            lbcur = lookbehind!(((*inp as usize) << 3) + ((inst >> 2) & 0x7) + 1);
            inp = inp.add(1);
            lblen = (inst >> 5) + 1;
            nstate = inst & 0x3;
        } else if inst & M3_MARKER != 0 {
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
                inp = inp.add(offset);
                needs_in!(1);
                lblen += offset * 255 + 31 + *inp as usize;
                inp = inp.add(1);
            }
            needs_in!(2);
            nstate = get_le16(inp);
            inp = inp.add(2);
            lbcur = lookbehind!((nstate >> 2) + 1);
            nstate &= 0x3;
        } else if inst & M4_MARKER != 0 {
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
                inp = inp.add(offset);
                needs_in!(1);
                lblen += offset * 255 + 7 + *inp as usize;
                inp = inp.add(1);
            }
            needs_in!(2);
            nstate = get_le16(inp);
            inp = inp.add(2);
            let lb_off = ((inst & 0x8) << 11) + (nstate >> 2);
            nstate &= 0x3;
            if lb_off == 0 {
                break; /* Stream finished */
            }
            lbcur = lookbehind!(lb_off + 16384);
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
                    inp = inp.add(offset);
                    needs_in!(1);
                    len += offset * 255 + 15 + *inp as usize;
                    inp = inp.add(1);
                }
                /* copy_literal_run */
                needs_in!(len);
                needs_out!(len);
                std::ptr::copy_nonoverlapping(inp, outp, len);
                inp = inp.add(len);
                outp = outp.add(len);
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
                lbcur = lookbehind!((inst >> 2) + ((*inp as usize) << 2) + 1);
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
                lbcur = lookbehind!((inst >> 2) + ((*inp as usize) << 2) + 2049);
                inp = inp.add(1);
                lblen = 3;
            }
        }

        needs_in!(nstate);
        needs_out!(lblen + nstate);
        /* Copy lookbehind */
        // NOTE: cannot use `copy_within`, as RLE depends on the
        // subsequent reads reading just overwritten data from
        // overlapping ranges
        let mut lb = lbcur as *mut u8;
        for _ in 0..lblen {
            *outp = *lb;
            outp = outp.add(1);
            lb = lb.add(1);
        }
        state = nstate;
        /* Copy literal */
        for _ in 0..nstate {
            *outp = *inp;
            outp = outp.add(1);
            inp = inp.add(1);
        }
    }

    let dst_size = outp as usize - dst_base as usize;
    /* Ensure terminating M4 was encountered */
    if lblen != 3 {
        return Err(EResult::Error);
    }
    if inp == inp_end {
        Ok(dst_size)
    } else if inp < inp_end {
        Err(EResult::InputNotConsumed(dst_size))
    } else {
        Err(EResult::InputOverrun)
    }
}

/// # Safety
///
/// `ptr` must point to at least 2 readable bytes.
#[inline(always)]
unsafe fn get_le16(ptr: *const u8) -> usize {
    let lsb = *ptr;
    let msb = *ptr.add(1);
    ((msb as usize) << 8) | lsb as usize
}

pub fn compress(src: &[u8], out: &mut [u8], dict: &mut Dict) -> Result<usize, EResult> {
    dict.reset();
    let mut s = State::new(src, dict);
    let mut outp = 0; // outp == dst in cpp means outp == 0 in rust
    let mut lit_len = 0;
    let mut lit_ptr = s.inp;
    let mut lb_len = 0;
    let mut lb_off = 0;
    let mut best_off = [0; MAX_MATCH_BY_LENGTH_LEN];

    dict.advance(&mut s, &mut lb_off, &mut lb_len, &mut best_off, false);

    while s.buf_sz > 0 {
        if lit_len == 0 {
            lit_ptr = s.bufp;
        }
        if lb_len < 2
            || (lb_len == 2 && (lb_off > M1_MAX_OFFSET || lit_len == 0 || lit_len >= 4))
            || (lb_len == 2 && outp == 0)
            || (outp == 0 && lit_len == 0)
        {
            lb_len = 0;
        } else if lb_len == M2_MIN_LEN && lb_off > M1_MAX_OFFSET + M2_MAX_OFFSET && lit_len >= 4 {
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
    needs_out(out, outp, 3)?;
    out[outp] = (M4_MARKER | 1) as u8;
    outp += 1;
    out[outp] = 0;
    outp += 1;
    out[outp] = 0;
    outp += 1;

    Ok(outp)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOREM: &[u8] = include_bytes!("../benches/lorem.txt");

    fn roundtrip(input: &[u8]) {
        let mut dict = Dict::new();
        let mut compressed = vec![0u8; input.len() + input.len() / 16 + 64 + 3];
        let compressed_len = compress(input, &mut compressed, &mut dict).unwrap();
        let compressed = &compressed[..compressed_len];

        let mut decompressed = vec![0u8; input.len()];
        let decompressed_len = decompress(compressed, &mut decompressed).unwrap();
        assert_eq!(decompressed_len, input.len());
        assert_eq!(&decompressed[..decompressed_len], input);
    }

    #[test]
    fn test_roundtrip_lorem() {
        roundtrip(LOREM);
    }

    #[test]
    fn test_roundtrip_all_zeros() {
        roundtrip(&[0u8; 65536]);
    }

    #[test]
    fn test_roundtrip_sequential() {
        let data: Vec<u8> = (0..=255).cycle().take(4096).collect();
        roundtrip(&data);
    }

    #[test]
    fn test_roundtrip_single_byte() {
        roundtrip(&[42]);
    }

    #[test]
    fn test_roundtrip_two_bytes() {
        roundtrip(&[1, 2]);
    }

    #[test]
    fn test_roundtrip_empty() {
        let mut dict = Dict::new();
        let mut compressed = vec![0u8; 67];
        let compressed_len = compress(&[], &mut compressed, &mut dict).unwrap();
        let compressed = &compressed[..compressed_len];

        let mut decompressed = vec![0u8; 0];
        // Verifying no UB on empty input; result may be Err depending on format
        let _ = decompress(compressed, &mut decompressed);
    }

    #[test]
    fn test_decompress_too_short() {
        assert_eq!(
            decompress(&[0, 0], &mut [0; 64]),
            Err(EResult::InputOverrun)
        );
    }

    #[test]
    fn test_decompress_output_too_small() {
        let mut dict = Dict::new();
        let mut compressed = vec![0u8; LOREM.len()];
        let compressed_len = compress(LOREM, &mut compressed, &mut dict).unwrap();

        let mut tiny = vec![0u8; 10];
        let result = decompress(&compressed[..compressed_len], &mut tiny);
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_garbage_does_not_panic() {
        // Must return Err, never panic or UB
        let garbage_inputs: &[&[u8]] = &[
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            &[0x00, 0x00, 0x00, 0x00, 0x00],
            &[0x11, 0x00, 0x00],
            &[22, 1, 2, 3, 4, 5],
        ];
        for input in garbage_inputs {
            let mut out = vec![0u8; 1024];
            let _ = decompress(input, &mut out);
        }
    }

    // Regression tests for malformed inputs that exercise edge cases in
    // decompress_inner: zero-byte-length scanning and lookbehind pointer
    // arithmetic.

    #[test]
    fn regression_m1_zero_scan_oob() {
        // M1 literal-run path: inst=0x00, state=0, len=3 triggers
        // zero-byte-length scanning that read past the input buffer.
        let _ = decompress(&[0x00, 0x00, 0x00, 0x00], &mut [0u8; 1024]);
    }

    #[test]
    fn regression_m3_zero_scan_oob() {
        // M3 path: first byte 0x12 copies 1 literal (state=1), then
        // inst=0x20 enters M3 with lblen=2, triggering zero scan on
        // trailing zeros that read past the input buffer.
        let _ = decompress(&[0x12, 0xAA, 0x20, 0x00], &mut [0u8; 1024]);
    }

    #[test]
    fn regression_m4_zero_scan_oob() {
        // M4 path: same setup, inst=0x10 enters M4 with lblen=2,
        // triggering zero scan past the input buffer.
        let _ = decompress(&[0x12, 0xAA, 0x10, 0x00], &mut [0u8; 1024]);
    }

    #[test]
    fn regression_m2_lookbehind_overrun() {
        // M2 path: inst=0xC0 computes lookbehind offset from next byte
        // (0xFF -> offset 2041). With only 1 byte of output, outp.sub(2041)
        // produced a dangling pointer before the bounds check.
        let _ = decompress(&[0x12, 0xAA, 0xC0, 0xFF], &mut [0u8; 1024]);
    }

    #[test]
    fn regression_m4_lookbehind_overrun() {
        // M4 terminator decoded into a zero-length output buffer: the
        // lookbehind pointer computation outp.sub(n) underflowed the
        // allocation, producing a dangling pointer.
        let mut dict = Dict::new();
        let compressed = {
            let mut buf = vec![0u8; 67];
            let len = compress(&[], &mut buf, &mut dict).unwrap();
            buf.truncate(len);
            buf
        };
        let _ = decompress(&compressed, &mut [0u8; 0]);
    }

    use arbtest::arbtest;

    #[test]
    fn prop_roundtrip() {
        arbtest(|u| {
            let data: Vec<u8> = u.arbitrary()?;
            if data.is_empty() {
                return Ok(());
            }
            let mut dict = Dict::new();
            let mut compressed = vec![0u8; data.len() + data.len() / 16 + 64 + 3];
            let compressed_len = compress(&data, &mut compressed, &mut dict).unwrap();

            let mut decompressed = vec![0u8; data.len()];
            let decompressed_len =
                decompress(&compressed[..compressed_len], &mut decompressed).unwrap();
            assert_eq!(&decompressed[..decompressed_len], &data[..]);
            Ok(())
        });
    }

    #[test]
    fn prop_decompress_arbitrary_no_ub() {
        arbtest(|u| {
            let data: Vec<u8> = u.arbitrary()?;
            let mut out = vec![0u8; 4096];
            // Must not panic or trigger UB — any Err is fine
            let _ = decompress(&data, &mut out);
            Ok(())
        });
    }
}
