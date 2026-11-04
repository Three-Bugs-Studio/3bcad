#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = threebcad_io::step::read_step(data);
});
