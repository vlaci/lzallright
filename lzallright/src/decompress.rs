use std::cell::Cell;

use super::consts::*;
use crate::error::{Error, ErrorKind};

/// Paired read and write cursors
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
    fn read_byte(&mut self) -> Result<usize, Error> {
        let byte = *self
            .src
            .get(self.inp)
            .ok_or_else(|| self.error(ErrorKind::InputOverrun))?;
        self.inp += 1;
        Ok(usize::from(byte))
    }

    /// Reads the next two input bytes as a little-endian integer.
    #[inline]
    fn read_le16(&mut self) -> Result<usize, Error> {
        let Some(&[lo, hi]) = self.src.get(self.inp..self.inp + 2) else {
            return Err(self.error(ErrorKind::InputOverrun));
        };
        self.inp += 2;
        Ok(usize::from(u16::from_le_bytes([lo, hi])))
    }

    /// Returns `base + <count of zero bytes> * 255 + byte`.
    #[inline]
    fn read_zero_byte_length(&mut self, base: usize) -> Result<usize, Error> {
        let zeros = self.src[self.inp..].iter().take_while(|&&b| b == 0).count();
        self.inp += zeros;
        if zeros > Max255Count {
            return Err(self.error(ErrorKind::Error));
        }
        Ok(zeros * 255 + base + self.read_byte()?)
    }

    /// Copies `len` bytes from the input to the output.
    #[inline]
    fn copy_literal(&mut self, len: usize) -> Result<(), Error> {
        let Some(lit) = self.src.get(self.inp..self.inp + len) else {
            return Err(self.error(ErrorKind::InputOverrun));
        };
        let Some(out) = self.dst.get_mut(self.outp..self.outp + len) else {
            return Err(Error::new(ErrorKind::OutputOverrun, self.outp));
        };
        out.copy_from_slice(lit);
        self.inp += len;
        self.outp += len;
        Ok(())
    }

    /// Copies `len` bytes from `dist` bytes behind the output position, then
    /// `literal_count` literals.
    ///
    /// `len` has to be at least 2 and `literal_count` at most 3.
    #[inline]
    fn copy_match(&mut self, dist: usize, len: usize, literal_count: usize) -> Result<(), Error> {
        debug_assert!(len >= 2 && literal_count <= 3);
        if dist > self.outp {
            return Err(self.error(ErrorKind::LookbehindOverrun));
        }
        let Some(lit) = self.src.get(self.inp..self.inp + literal_count) else {
            return Err(self.error(ErrorKind::InputOverrun));
        };
        let Some(window) = self
            .dst
            .get_mut(self.outp - dist..self.outp + len + literal_count)
        else {
            return Err(Error::new(ErrorKind::OutputOverrun, self.outp));
        };
        let (window, out) = window.split_at_mut(dist + len);

        if dist >= 8 {
            // PERF: the farther a match, the wider blocks we can use to copy
            copy_words(window, dist, len);
        } else {
            // Has to copy one byte at a time if we are closer than a word due to RLE
            let window = Cell::from_mut(window).as_slice_of_cells();
            for (d, s) in window[dist..].iter().zip(window) {
                d.set(s.get());
            }
        }

        // Two overlapping 2-byte copies cover 2 or 3 literals without a
        // branch on the exact count.
        if literal_count >= 2 {
            let tail = literal_count - 2;
            out[..2].copy_from_slice(&lit[..2]);
            out[tail..].copy_from_slice(&lit[tail..]);
        } else if literal_count == 1 {
            out[0] = lit[0];
        }
        self.inp += literal_count;
        self.outp += len + literal_count;
        Ok(())
    }
}

/// Copies `window[..len]` to `window[dist..]` in fixed-width blocks no wider
/// than `dist`, so no block reads a byte an earlier block of the same copy
/// has yet to write. Requires `dist >= 8`.
#[inline(always)]
fn copy_words(window: &mut [u8], dist: usize, len: usize) {
    debug_assert!(dist >= 8);
    macro_rules! copy_block {
        ($n:literal, $at:expr) => {{
            let at = $at;
            let block: [u8; $n] = window[at..at + $n].try_into().unwrap();
            window[dist + at..dist + at + $n].copy_from_slice(&block);
        }};
    }
    macro_rules! copy_blocks {
        ($n:literal) => {{
            let mut at = 0;
            while at + $n <= len {
                copy_block!($n, at);
                at += $n;
            }
            if at < len {
                copy_block!($n, len - $n);
            }
        }};
    }
    if len >= 8 {
        if dist >= 16 && len >= 16 {
            copy_blocks!(16);
        } else {
            copy_blocks!(8);
        }
    } else if len >= 4 {
        copy_block!(4, 0);
        copy_block!(4, len - 4);
    } else {
        copy_block!(2, 0);
        copy_block!(2, len - 2);
    }
}

/// Decompresses the LZO stream `src` into `dst` and returns the number of
/// bytes written.
///
/// # Examples
///
/// ```
/// # let mut dict = lzallright::Dict::new();
/// # let mut input = vec![0; 4];
/// # let len = lzallright::compress(b"", &mut input, &mut dict).unwrap();
/// # input.resize(len, 0);
/// // let input = ...
/// let mut output = vec![0; 64];
/// let size = lzallright::decompress(&input, &mut output).unwrap();
/// ```
///
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

    #[test]
    fn copy_match_writes_exactly_len_literals() {
        // There are possible performance optimizations that would leave garbage at the end of output buffer.
        // As of now, we don't want to allow that.
        const GUARD: u8 = 0xA5;
        let prefix: Vec<u8> = (1..=64).collect();
        let lits = [0xF1, 0xF2, 0xF3];

        for dist in 1..=40 {
            for len in 2..=40 {
                for literal_count in 0..=3 {
                    let end = prefix.len() + len + literal_count;
                    let mut dst = vec![GUARD; end + 16];
                    dst[..prefix.len()].copy_from_slice(&prefix);
                    let mut decoder = Decoder::new(&lits[..literal_count], &mut dst);
                    decoder.outp = prefix.len();

                    decoder.copy_match(dist, len, literal_count).unwrap();
                    assert_eq!((decoder.inp, decoder.outp), (literal_count, end));

                    let mut expected = prefix.clone();
                    for _ in 0..len {
                        expected.push(expected[expected.len() - dist]);
                    }
                    expected.extend_from_slice(&lits[..literal_count]);
                    let case = format!("dist {dist}, len {len}, literals {literal_count}");
                    assert_eq!(dst[..end], expected, "{case}");
                    assert!(
                        dst[end..].iter().all(|&b| b == GUARD),
                        "wrote past output: {case}"
                    );
                }
            }
        }
    }
}
