use super::consts::*;

/// Circular buffer caching enough data to access the maximum lookback
/// distance of 48K + maximum match length of 2K. An additional 2K is
/// allocated so the start of the buffer may be replicated at the end,
/// therefore providing efficient circular access.
pub struct Window {
    buf: [u8; BUF_SIZE + MAX_MATCH_LEN],
    pos: usize,
    write_pos: usize,
    lookahead_len: usize,
    /// Set once `write_pos` wraps around. After that, every push
    /// overwrites a previous entry.
    full: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            buf: [0; BUF_SIZE + MAX_MATCH_LEN],
            pos: 0,
            write_pos: 0,
            lookahead_len: 0,
            full: false,
        }
    }
}

impl Window {
    /// Starts a new input: loads up to `MAX_MATCH_LEN` bytes of `src` as the
    /// first lookahead and returns an iterator over the rest, which callers
    /// feed back one byte per `push`.
    ///
    /// Bytes left over from a previous input stay in the buffer. They are
    /// unreachable because the matchers are reset at the same time.
    pub fn reset<'a>(&mut self, src: &'a [u8]) -> std::slice::Iter<'a, u8> {
        let (head, rest) = src.split_at(src.len().min(MAX_MATCH_LEN));
        self.buf[..head.len()].copy_from_slice(head);
        if head.len() < 3 {
            // See `make_key` why
            self.buf[head.len()..head.len() + 3].fill(0);
        }
        self.pos = 0;
        self.write_pos = head.len();
        self.lookahead_len = head.len();
        self.full = false;
        rest.iter()
    }

    /// Returns the current position, the buffer index of the first lookahead byte.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Returns the unconsumed input starting at the current position.
    pub fn lookahead(&self) -> &[u8] {
        &self.buf[self.pos..self.pos + self.lookahead_len]
    }

    /// Returns the buffer from `pos` to its end, at least `MAX_MATCH_LEN` long.
    pub fn bytes_from(&self, pos: usize) -> &[u8] {
        &self.buf[pos..]
    }

    /// Returns how many bytes `pos` lies behind the current position, wrapping around the buffer.
    pub fn behind(&self, pos: usize) -> usize {
        if self.pos > pos {
            self.pos - pos
        } else {
            BUF_SIZE - (pos - self.pos)
        }
    }

    /// Position the next `push` overwrites, once the window is full.
    pub fn evicting(&self) -> Option<usize> {
        self.full.then_some(self.write_pos)
    }

    /// Appends the next input byte, or 0 and shrinks the lookahead past the
    /// end of input, then moves to the next position.
    pub fn push(&mut self, byte: Option<u8>) {
        let byte = byte.unwrap_or_else(|| {
            self.lookahead_len -= 1;
            0
        });
        self.buf[self.write_pos] = byte;

        if self.write_pos < MAX_MATCH_LEN {
            self.buf[BUF_SIZE + self.write_pos] = byte;
        }

        self.write_pos += 1;
        if self.write_pos == BUF_SIZE {
            self.write_pos = 0;
            self.full = true;
        }

        self.pos += 1;
        if self.pos == BUF_SIZE {
            self.pos = 0;
        }
    }
}
