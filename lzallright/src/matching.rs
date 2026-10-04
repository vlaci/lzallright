use super::consts::*;
use super::window::Window;

/// Working memory of the compressor: both hash tables and the window,
/// several hundred kilobytes in all. It is safe to re-use it between
/// `compress` calls to reduce allocations.
#[derive(Default)]
pub struct Dict {
    match3: Match3,
    match2: Match2,
    window: Window,
}

impl Default for Dict {
    fn default() -> Self {
        Self::new()
    }
}

/// One pass of the match finder over a single input.
pub struct Cursor<'d, 's> {
    dict: &'d mut Dict,
    input: std::slice::Iter<'s, u8>,
}

impl Cursor<'_, '_> {
    /// Returns the best match at the current position and moves one byte forward, or `None`
    /// at the end of input.
    #[inline(always)]
    pub fn advance(&mut self) -> Option<LookbackMatch> {
        self.dict.advance(&mut self.input)
    }

    /// Moves `n` bytes forward, indexing the skipped positions without searching them.
    #[inline(always)]
    pub fn skip(&mut self, n: usize) {
        self.dict.skip(n, &mut self.input)
    }
}

impl Dict {
    pub fn new() -> Self {
        Default::default()
    }

    /// Clears the hash tables, loads the start of `src` into the
    /// window, and returns a cursor over the rest.
    pub(crate) fn start<'a>(&mut self, src: &'a [u8]) -> Cursor<'_, 'a> {
        self.match3.reset();
        self.match2.reset();
        let input = self.window.reset(src);
        Cursor { dict: self, input }
    }

    /// Drops the position the window's next push overwrites from both
    /// hash tables, once it has filled.
    fn evict(&mut self) {
        if let Some(pos) = self.window.evicting() {
            let bytes = self.window.bytes_from(pos);
            self.match3.remove(Match3::make_key(bytes));
            self.match2.remove(pos, Match2::make_key(bytes));
        }
    }

    /// Indexes the next `n` positions without searching for matches.
    fn skip(&mut self, n: usize, input: &mut std::slice::Iter<u8>) {
        for _ in 0..n {
            let pos = self.window.pos();
            let cur = self.window.bytes_from(pos);
            let (key3, key2) = (Match3::make_key(cur), Match2::make_key(cur));
            self.evict();
            self.match3.skip_advance(pos, key3);
            self.match2.add(pos, key2);
            self.window.push(input.next().copied());
        }
    }

    /// Finds the best match at the current position and moves past it;
    /// `None` once the input is exhausted.
    fn advance(&mut self, input: &mut std::slice::Iter<u8>) -> Option<LookbackMatch> {
        let sz = self.window.lookahead().len();
        if sz == 0 {
            return None;
        }
        let mut lb = LookbackMatch::new();
        let pos = self.window.pos();
        let cur = self.window.bytes_from(pos);
        let (key3, key2) = (Match3::make_key(cur), Match2::make_key(cur));

        let (mut match_pos, match_count) = self.match3.advance(pos, key3);

        if sz == 1 {
            self.match3.best_len[pos] = MAX_MATCH_LEN as u16 + 1;
        } else {
            let mut best_pos = [0u16; MaxMatchByLengthLen];
            if let Some(p) = self.match2.search(key2) {
                best_pos[2] = p as u16 + 1;
                lb.len = 2;
                let mut lb_pos = p;
                if sz >= 3 {
                    let wind = self.window.lookahead();
                    for _ in 0..match_count {
                        let match_len = mismatch(wind, self.window.bytes_from(match_pos));

                        if match_len < 2 {
                            match_pos = self.match3.chain[match_pos] as usize;
                            continue;
                        }
                        if match_len < MaxMatchByLengthLen && best_pos[match_len] == 0 {
                            best_pos[match_len] = match_pos as u16 + 1;
                        }
                        if match_len > lb.len {
                            lb.len = match_len;
                            lb_pos = match_pos;
                            if match_len == sz
                                || match_len > self.match3.best_len[match_pos] as usize
                            {
                                break;
                            }
                        }
                        match_pos = self.match3.chain[match_pos] as usize;
                    }
                }
                lb.off = self.window.behind(lb_pos);
            }
            self.match3.best_len[pos] = lb.len as u16;
            // `find_better_match` only reads lengths `len - 2..=len`, and only
            // for `M2MinLen < len <= MaxMatchByLengthLen`, `off > M2MaxOffset`.
            // `len == MaxMatchByLengthLen` leaves `best_off[2]` unread and unset.
            if (M2MinLen + 1..=MaxMatchByLengthLen).contains(&lb.len) && lb.off > M2MaxOffset {
                let lens = lb.len - 2..lb.len.min(MaxMatchByLengthLen - 1) + 1;
                for (off, &pos) in lb.best_off.iter_mut().zip(&best_pos[lens]) {
                    *off = match pos {
                        0 => 0,
                        pos => self.window.behind(usize::from(pos) - 1),
                    };
                }
            }
        }

        self.evict();
        self.match2.add(pos, key2);
        self.window.push(input.next().copied());
        Some(lb)
    }
}

/// Hash chains over 3-byte prefixes. `head[key]` holds the newest position
/// whose next three bytes hash to `key`, and `chain[pos]` links each position
/// to the previous one with the same hash. `chain_sz[key]` counts how many of
/// a chain's positions are still in the window, which bounds every walk so it
/// never reaches an evicted entry. `best_len[pos]` records the longest match
/// found when `pos` itself was searched; a later search ends early when it
/// beats that length at `pos`, a heuristic taken from lzokay.
struct Match3 {
    head: [u16; HASH_SIZE],
    chain_sz: [u16; HASH_SIZE],
    chain: [u16; BUF_SIZE],
    best_len: [u16; BUF_SIZE],
}

impl Match3 {
    /// Hashes the first three bytes of `data` into a 14-bit key.
    fn make_key(data: &[u8]) -> usize {
        let intermediate = ((usize::from(data[0]) << 5) ^ usize::from(data[1])).wrapping_shl(5)
            ^ usize::from(data[2]);

        intermediate.wrapping_mul(0x9F5F).wrapping_shr(5) & 0x3FFF
    }
    /// Returns the newest position for `key`, or `u16::MAX` when its chain is empty.
    const fn get_head(&self, key: usize) -> u16 {
        if self.chain_sz[key] == 0 {
            u16::MAX
        } else {
            self.head[key]
        }
    }
    /// Shortens the chain of `key` by its oldest position, which is about to leave the window.
    fn remove(&mut self, key: usize) {
        let c = &mut self.chain_sz[key];
        *c = c.wrapping_sub(1);
    }

    /// Adds `pos` to the chain of `key` and returns the previous head and the number of
    /// candidates to walk.
    fn advance(&mut self, pos: usize, key: usize) -> (usize, usize) {
        let head = self.get_head(key);
        let match_pos = head;
        self.chain[pos] = head;
        let mut match_count = self.chain_sz[key];
        self.chain_sz[key] += 1;
        if match_count > MAX_MATCH_LEN as u16 {
            match_count = MAX_MATCH_LEN as u16
        }
        self.head[key] = pos as u16;
        (match_pos as usize, match_count as usize)
    }

    /// Adds `pos` to the chain of `key` without searching.
    fn skip_advance(&mut self, pos: usize, key: usize) {
        self.chain[pos] = self.get_head(key);
        self.head[key] = pos as _;
        self.best_len[pos] = MAX_MATCH_LEN as u16 + 1;
        let c = &mut self.chain_sz[key];
        *c = c.wrapping_add(1);
    }

    /// Empties every chain.
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

/// Newest position of every 2-byte prefix, in a table indexed by the two
/// bytes themselves. A 2-byte match is viable only at short distances, so the
/// newest occurrence is the only one worth keeping; `u16::MAX` marks an empty
/// slot.
struct Match2 {
    head: [u16; 1 << 16],
}

impl Match2 {
    /// Packs the first two bytes of `data` into a table index.
    fn make_key(data: &[u8]) -> usize {
        (data[0] as u16 ^ ((data[1] as u16) << 8)) as usize
    }

    /// Records `pos` as the newest position for `key`.
    fn add(&mut self, pos: usize, key: usize) {
        self.head[key] = pos as u16;
    }
    /// Clears the entry for `key` if it still points at `pos`.
    fn remove(&mut self, pos: usize, key: usize) {
        let p = &mut self.head[key];
        if *p == pos as u16 {
            *p = u16::MAX
        }
    }

    /// Returns the newest earlier position for `key`, if any.
    fn search(&self, key: usize) -> Option<usize> {
        let pos = self.head[key];
        if pos == u16::MAX {
            return None;
        }
        Some(usize::from(pos))
    }

    /// Empties the table.
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

/// Best earlier match at the current position.
pub struct LookbackMatch {
    len: usize,
    off: usize,
    /// Offsets of the nearest matches of length `len - 2` to `len` or 0
    best_off: [usize; 3],
}

impl LookbackMatch {
    fn new() -> Self {
        Self {
            len: 0,
            off: 0,
            best_off: [0; 3],
        }
    }

    /// Returns how many bytes the match copies.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns how far back the match starts.
    pub fn offset(&self) -> usize {
        self.off
    }

    /// Swaps in a match one or two bytes shorter when its smaller offset gives a cheaper
    /// instruction.
    pub fn find_better_match(&mut self) {
        if self.len <= M2MinLen || self.off <= M2MaxOffset {
            return;
        }
        // Offset of the nearest match of length `n` (`len - 2 <= n <= len`).
        let len = self.len;
        let off_of = |n: usize| self.best_off[n + 2 - len];
        if self.len <= M2MaxLen + 1
            && off_of(self.len - 1) != 0
            && off_of(self.len - 1) <= M2MaxOffset
        {
            self.len -= 1;
        } else if self.off > M3MaxOffset
            && self.len > M4MaxLen
            && self.len <= M2MaxLen + 2
            && off_of(self.len - 2) != 0
            && off_of(self.len) <= M2MaxOffset
        {
            self.len -= 2;
        } else if self.off > M3MaxOffset
            && self.len > M4MaxLen
            && self.len <= M3MaxLen + 1
            && off_of(self.len - 1) != 0
            && off_of(self.len - 2) <= M3MaxOffset
        {
            self.len -= 1;
        } else {
            return;
        }
        self.off = off_of(self.len);
    }
}

/// Returns the length of the common prefix of `a` and `b`.
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
}
