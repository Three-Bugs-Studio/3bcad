#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = threebcad_io::dxf::read_dxf(data, None);
});
