//! Raster bytes for the tracer (PNG/JPEG): `burnburn_image::decode` with default limits gives a
//! typed `DecodeError` or an image, never a panic or an unbounded allocation (request Trace).
//! Seed the corpus with `test-data/trace/synthetic/*.png`.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = burnburn_image::decode(data, burnburn_image::DecodeLimits::default());
    let _ = burnburn_io::import::inspect_raster(data, &burnburn_io::import::RasterLimits::default());
});
