#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Parser must never panic. Any Err is fine; any Ok must also not panic.
        let _ = cet_dsl::parse_query("fuzz", s, 0, 0);
    }
});
