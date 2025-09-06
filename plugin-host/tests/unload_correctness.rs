use std::path::PathBuf;

// This test verifies that plugin-side unmaker code runs by calling the
// aggregated `plugin_unregister_all_Greeter_v1` helper and then reading the
// plugin-exported `UNMAKER_COUNTER` static before unloading the library.
#[test]
fn unload_and_reload_plugin() {
    // Path to the compiled plugin library (same as before).
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../plugins/plugin-multi/target/debug");
    #[cfg(target_os = "windows")]
    path.push("plugin_multi.dll");
    #[cfg(not(target_os = "windows"))]
    path.push("libplugin_multi.so");

    // Load the library and obtain the registration array
    let (lib, arr_ptr) =
        plugin_interface::load_greeter_from_lib(&path).expect("failed to load plugin");

    // call greet on first registration (same logic as PluginManager)
    unsafe {
        let arr = &*arr_ptr;
        let slice = std::slice::from_raw_parts(arr.registrations, arr.count);
        let reg = &*(slice[0] as *const plugin_interface::GreeterRegistration);
        let vtable = &*reg.vtable;
        let c_target = std::ffi::CString::new("world").unwrap();
        (vtable.greet)(vtable.user_data, c_target.as_ptr());
    }

    // Call the plugin's bulk-unregister helper (if present). This will run
    // the generated unregister_all which calls each factory.unmaker and thus
    // increments the crate's `UNMAKER_COUNTER`.
    unsafe {
        if let Ok(unreg_all) = lib
            .get::<unsafe extern "C" fn(*const plugin_interface::RegistrationArray)>(
                b"plugin_unregister_all_Greeter_v1",
            )
        {
            unreg_all(arr_ptr);

            // Call the versioned getter to obtain the current counter value (u64) and assert > 0
            if let Ok(getter_sym) =
                lib.get::<unsafe extern "C" fn() -> u64>(b"plugin_unmaker_counter_Greeter_v1")
            {
                let val = getter_sym();
                assert!(val > 0, "UNMAKER_COUNTER was not incremented by unmaker");
            } else {
                panic!("plugin did not export plugin_unmaker_counter_Greeter_v1");
            }
        } else {
            panic!("plugin did not export plugin_unregister_all_Greeter_v1");
        }

        // Finally, drop the library (unload)
        drop(lib);
    }

    // Reload should succeed
    let (_lib2, _arr2) = plugin_interface::load_greeter_from_lib(&path).expect("reload failed");
}
