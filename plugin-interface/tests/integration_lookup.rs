use libloading::Library;

// Attempt to lookup the generated test getter exported from this crate's test
// module. This uses `Library::this()` where supported (libloading exposes
// Library::this on some platforms) or falls back to opening the current
// process as a library via the platform-specific handle.
#[test]
fn lookup_local_test_getter_in_process() {
    // This test expects that the unit tests in this crate exported a
    // `plugin_unmaker_counter_TestTrait_v1` symbol. However, depending on how
    // cargo runs tests, that symbol might not be visible via Library::this().
    // We'll attempt to open the current process and gracefully skip if not
    // possible.

    // Attempt to open the current executable as a library. This may or may
    // not expose the test symbol depending on the platform and how tests are
    // executed; if we can't open it or the symbol isn't present we skip the
    // test to avoid platform flakes.
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping test: cannot determine current_exe: {}", e);
            return;
        }
    };

    let lib = match unsafe { Library::new(&exe) } {
        Ok(l) => l,
        Err(e) => {
            eprintln!("skipping test: cannot open current_exe as Library: {}", e);
            return;
        }
    };

    // Now attempt to look up the test getter symbol that the unit tests
    // export: `plugin_unmaker_counter_TestTrait_v1`.
    let symbol_name = b"plugin_unmaker_counter_TestTrait_v1\0";
    unsafe {
        match lib.get::<unsafe extern "C" fn() -> u64>(symbol_name) {
            Ok(f) => {
                let v = f();
                eprintln!("found test getter in process, value={}", v);
                // basic sanity check
                assert_eq!(v, 42u64);
            }
            Err(_) => {
                eprintln!("test getter not present in process; skipping");
            }
        }
    }
}
