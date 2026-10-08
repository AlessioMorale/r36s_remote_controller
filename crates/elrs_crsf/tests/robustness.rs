//! Stable-toolchain stand-in for the cargo-fuzz target (fuzz/): random and mutated input
//! through the parser and every decoder must never crash, and decoded frames re-encode.

use elrs_crsf::{decode, decode_bytes, parse_param_entry, reencode, Frame, Parser};

/// xorshift64*, deterministic so failures reproduce
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn byte(&mut self) -> u8 {
        (self.next() >> 56) as u8
    }
}

fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |mut crc, b| {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0xD5 } else { crc << 1 };
        }
        crc
    })
}

#[test]
fn random_bytes_never_crash() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let mut parser = Parser::new();
    for _ in 0..2_000 {
        let len = (rng.next() % 300) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
        for frame in parser.feed(&bytes) {
            let _ = decode(&frame);
            let _ = reencode(&frame.bytes);
        }
        let _ = decode_bytes(&bytes);
        let _ = reencode(&bytes);
        let _ = parse_param_entry(rng.byte(), &bytes);
    }
}

#[test]
fn valid_crc_random_payloads_never_crash() {
    // Random payloads with a correct CRC reach the type-specific deserializers
    let mut rng = Rng(42);
    let types = [0x08, 0x14, 0x16, 0x21, 0x28, 0x29, 0x2B, 0x2C, 0x2D, 0x32, 0x3A];
    let syncs = [0xC8, 0xEA, 0xEE, 0x00];
    let mut parser = Parser::new();
    for i in 0..50_000 {
        let payload_len = (rng.next() % 61) as usize;
        let mut body = vec![types[i % types.len()]];
        body.extend((0..payload_len).map(|_| rng.byte()));
        let mut frame = vec![syncs[i % syncs.len()], (body.len() + 1) as u8];
        frame.extend(&body);
        frame.push(crc8(&body));
        let frames = parser.feed(&frame);
        assert_eq!(frames.len(), 1, "frame {i} not parsed: {frame:02x?}");
        let _ = decode(&frames[0]);
        let _ = parse_param_entry(0, &body);
        if let Some(out) = reencode(&frame) {
            if let (Frame::RcChannels(a), Frame::RcChannels(b)) =
                (decode_bytes(&out), decode_bytes(&reencode(&out).unwrap()))
            {
                // The library converts 11-bit ticks to whole µs (truncating), so RC values may
                // move by 1 µs per round trip; everything else must be stable
                assert!(a.iter().zip(b.iter()).all(|(x, y)| x.abs_diff(*y) <= 1), "{a:?} {b:?}");
            } else {
                assert_eq!(reencode(&out).as_deref(), Some(out.as_slice()));
            }
        }
    }
    assert_eq!(parser.stats().crc_errors, 0);
}
