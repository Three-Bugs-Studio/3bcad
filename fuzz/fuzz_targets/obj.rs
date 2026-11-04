#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = threebcad_io::mesh::parse_obj(data);
});
