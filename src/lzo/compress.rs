use super::consts::*;
use super::matching::Dict;
use crate::error::{Error, ErrorKind};

/// Bounds-checked write cursor over the output buffer that knows LZO
/// instruction encoding. Callers have to `reserve` room before
/// writing. `write_byte` itself never fails.  A failed reservation
/// reports `pos` as the amount of output produced so far.
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

    /// Writes one byte into room the caller has already reserved.
    #[inline]
    fn write_byte(&mut self, byte: u8) {
        self.buf[self.pos] = byte;
        self.pos += 1;
    }

    /// Writes `len` as LZO's variable-length count: a zero byte for every 255,
    /// then the remainder.
    fn write_zero_byte_length(&mut self, mut len: usize) {
        while len > 255 {
            self.write_byte(0);
            len -= 255;
        }
        self.write_byte(len as u8);
    }

    /// Writes the length of the literal run `lit`. Counts up to 3 are
    /// merged into the previous instruction.
    fn encode_literal_run(&mut self, lit: &[u8]) -> Result<(), Error> {
        let lit_len = lit.len();
        if lit_len == 0 {
            // Mustn't encode anything if there is nothing to write
            return Ok(());
        }
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

    /// Writes the shortest instruction that copies `lb_len` bytes from `lb_off`
    /// back, where `last_lit_len` decides whether M1's 3-byte form is available.
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

/// Compresses `src` into `out`, using `dict` as working memory, and returns the
/// compressed length.
pub fn compress(src: &[u8], out: &mut [u8], dict: &mut Dict) -> Result<usize, Error> {
    let mut input = dict.start(src);
    let mut writer = Writer::new(out);
    let mut lit_len = 0;
    // Index in `src` of the position the current `lb` was found at.
    let mut pos = 0;

    while let Some(mut lb) = input.advance() {
        let (len, off) = (lb.len(), lb.offset());
        // A match must come after at least one literal at stream start, and
        // short matches only worth to encode when the instruction is cheap.
        let reject = len < 2
            || (len == 2 && (off > M1MaxOffset || lit_len == 0 || lit_len >= 4 || writer.pos == 0))
            || (writer.pos == 0 && lit_len == 0)
            || (len == M2MinLen && off > M1MaxOffset + M2MaxOffset && lit_len >= 4);
        if reject {
            lit_len += 1;
            pos += 1;
            continue;
        }
        lb.find_better_match();
        writer.encode_literal_run(&src[pos - lit_len..pos])?;
        writer.encode_lookback_match(lb.len(), lb.offset(), lit_len)?;
        lit_len = 0;
        input.skip(lb.len() - 1);
        pos += lb.len();
    }

    writer.encode_literal_run(&src[pos - lit_len..pos])?;

    // Terminating M4
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
}
