//! Fuzzes the C++ parser through the facade (design §3.5 safety boundary): arbitrary
//! serial bytes go through `parser_feed`, and every frame it yields through every decoder,
//! the re-encoder, and the parameter-entry parser.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut parser = elrs_crsf::Parser::new();
    // Split the input in two feeds so frames that straddle reads are covered
    let split = data.first().map_or(0, |b| *b as usize % (data.len() + 1));
    for part in [&data[..split], &data[split..]] {
        for frame in parser.feed(part) {
            let _ = elrs_crsf::decode(&frame);
            let _ = elrs_crsf::reencode(&frame.bytes);
            if frame.bytes.len() > 3 {
                let _ = elrs_crsf::parse_param_entry(frame.bytes[3], &frame.bytes[3..]);
            }
        }
    }
    let _ = elrs_crsf::decode_bytes(data);
    let _ = elrs_crsf::parse_param_entry(0, data);
});
