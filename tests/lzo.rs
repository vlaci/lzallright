use lzallright::error::ErrorKind;
use lzallright::lzo::{compress, decompress, Dict};

fn noise(len: usize, alphabet: u64) -> Vec<u8> {
    let mut x = 0x9E37_79B9_7F4A_7C15u64 ^ len as u64;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % alphabet) as u8
        })
        .collect()
}

const ALICE: &[u8] = include_bytes!("../benches/corpus/alice-pg11.txt");
const NASA: &[u8] = include_bytes!("../benches/corpus/nasa-http-jul95.log");
const SQLITE: &[u8] = include_bytes!("../benches/corpus/sqlite-btree.c");
const WIKIDATA: &[u8] = include_bytes!("../benches/corpus/wikidata-Q100020.json");

/// Compresses into an owned buffer.
fn lzo_compress(data: &[u8], dict: &mut Dict) -> Vec<u8> {
    let mut out = vec![0; data.len() + data.len() / 16 + 64 + 3];
    let len = compress(data, &mut out, dict).expect("worst-case bound suffices");
    out.truncate(len);
    out
}

fn roundtrip(data: &[u8]) {
    let compressed = lzo_compress(data, &mut Dict::new());

    let mut out = vec![0; data.len()];
    assert_eq!(decompress(&compressed, &mut out).unwrap(), data.len());
    assert!(out == data, "round-trip mismatch");

    if !data.is_empty() {
        let err = decompress(&compressed, &mut vec![0; data.len() - 1]).unwrap_err();
        assert_eq!(err.kind(), &ErrorKind::OutputOverrun);
    }
}

/// A dict that compressed a window-filling input first must not leak state.
fn reused_dict_matches_fresh_dict(data: &[u8]) {
    let mut reused = Dict::new();
    lzo_compress(&noise(60_000, 256), &mut reused);
    assert!(lzo_compress(data, &mut reused) == lzo_compress(data, &mut Dict::new()));
}

macro_rules! inputs {
    ($($name:ident => $data:expr,)*) => {$(
        mod $name {
            fn data() -> Vec<u8> {
                #[allow(unused_imports)]
                use super::*;
                $data
            }

            #[test]
            fn roundtrip() {
                super::roundtrip(&data());
            }

            #[test]
            fn reused_dict_matches_fresh_dict() {
                super::reused_dict_matches_fresh_dict(&data());
            }
        }
    )*};
}

// Literal header limits (238/239), window size (48K) and window + max match (50K).
inputs! {
    alice => ALICE.to_vec(),
    nasa => NASA.to_vec(),
    sqlite => SQLITE.to_vec(),
    wikidata => WIKIDATA.to_vec(),
    corpus_concat => [ALICE, NASA, SQLITE, WIKIDATA].concat(),
    zeros_1m => vec![0; 1 << 20],
    random_300k => noise(300_000, 256),
    low_entropy_0 => noise(0, 4),
    low_entropy_1 => noise(1, 4),
    low_entropy_2 => noise(2, 4),
    low_entropy_3 => noise(3, 4),
    low_entropy_4 => noise(4, 4),
    low_entropy_17 => noise(17, 4),
    low_entropy_18 => noise(18, 4),
    low_entropy_19 => noise(19, 4),
    low_entropy_238 => noise(238, 4),
    low_entropy_239 => noise(239, 4),
    low_entropy_49151 => noise(49_151, 4),
    low_entropy_49152 => noise(49_152, 4),
    low_entropy_49153 => noise(49_153, 4),
    low_entropy_51200 => noise(51_200, 4),
    low_entropy_1m => noise(1 << 20, 4),
}

fn decode(src: &[u8]) -> Result<Vec<u8>, (ErrorKind, usize)> {
    let mut out = vec![0; 1 << 16];
    match decompress(src, &mut out) {
        Ok(n) => Ok(out[..n].to_vec()),
        Err(e) => Err((e.kind().clone(), e.dst_size())),
    }
}

/// Opcode for a literal run of `n >= 19` bytes at the start of a stream.
fn long_literal(n: usize) -> Vec<u8> {
    let mut v = vec![0];
    let mut rest = n - 18;
    while rest > 255 {
        v.push(0);
        rest -= 255;
    }
    v.push(rest as u8);
    v
}

const EOS: [u8; 3] = [0x11, 0, 0];

#[test]
fn empty_stream_is_bare_terminator() {
    assert_eq!(lzo_compress(&[], &mut Dict::new()), EOS);
    assert_eq!(decode(&EOS), Ok(vec![]));
}

#[test]
fn first_byte_18_to_21_copies_up_to_four_literals() {
    assert_eq!(decode(&[0x13, b'a', b'b', 0x11, 0, 0]), Ok(b"ab".to_vec()));
}

#[test]
fn m1_after_short_literal_run_copies_two_bytes_within_1k() {
    // M1 D=1 S=1: copy 2 bytes from distance 2, then one literal.
    assert_eq!(
        decode(&[0x13, b'a', b'b', 0x05, 0, b'c', 0x11, 0, 0]),
        Ok(b"ababc".to_vec())
    );
}

#[test]
fn m1_after_long_literal_run_copies_three_bytes_from_2k() {
    let data = noise(2049, 256);
    let stream = [long_literal(2049), data.clone(), vec![0, 0], EOS.to_vec()].concat();
    let expected = [&data[..], &data[..3]].concat();
    assert_eq!(decode(&stream), Ok(expected));
}

#[test]
fn m3_extended_length() {
    // Length 2 + 31 + 255 + 1 = 289 from distance 2.
    let stream = [0x13, b'a', b'b', 0x20, 0, 1, 0x04, 0, 0x11, 0, 0];
    let mut expected = b"ab".repeat(146);
    expected.truncate(2 + 289);
    assert_eq!(decode(&stream), Ok(expected));
}

#[test]
fn m4_high_distance_bit() {
    // H=1, D=0: distance 32768.
    let data = noise(32768, 256);
    let stream = [
        long_literal(32768),
        data.clone(),
        vec![0x19, 0, 0],
        EOS.to_vec(),
    ]
    .concat();
    let expected = [&data[..], &data[..3]].concat();
    assert_eq!(decode(&stream), Ok(expected));
}

#[test]
fn terminator_with_wrong_length_is_an_error() {
    assert_eq!(
        decode(&[0x12, b'a', 0x12, 0, 0]),
        Err((ErrorKind::Error, 1))
    );
}

#[test]
fn trailing_input_reports_decoded_size() {
    assert_eq!(
        decode(&[0x12, b'a', 0x11, 0, 0, 0xff]),
        Err((ErrorKind::InputNotConsumed, 1))
    );
}

// Inputs that crashed the C++ lzokay decoder under `cargo fuzz decompress`
// (out-of-bounds reads in zero-byte-length scanning and lookbehind arithmetic).

#[test]
fn fuzz_crash_m1_zero_scan_oob() {
    let _ = decompress(&[0x00, 0x00, 0x00, 0x00], &mut [0u8; 1024]);
}

#[test]
fn fuzz_crash_m3_zero_scan_oob() {
    let _ = decompress(&[0x12, 0xAA, 0x20, 0x00], &mut [0u8; 1024]);
}

#[test]
fn fuzz_crash_m4_zero_scan_oob() {
    let _ = decompress(&[0x12, 0xAA, 0x10, 0x00], &mut [0u8; 1024]);
}

#[test]
fn fuzz_crash_m2_lookbehind_overrun() {
    let _ = decompress(&[0x12, 0xAA, 0xC0, 0xFF], &mut [0u8; 1024]);
}

#[test]
fn fuzz_crash_m4_lookbehind_overrun() {
    let mut dict = Dict::new();
    let compressed = {
        let mut buf = vec![0u8; 67];
        let len = compress(&[], &mut buf, &mut dict).expect("Compress failed");
        buf.truncate(len);
        buf
    };
    let _ = decompress(&compressed, &mut [0u8; 0]);
}
