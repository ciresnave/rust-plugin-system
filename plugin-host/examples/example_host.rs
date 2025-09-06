// Example kept: direct artifact loader (useful for narrow tests).
// Additionally provide a manager-owned watcher example below.

use plugin_interface::{load_greeter_from_lib, GreeterRegistration};
use std::ffi::CStr;
use std::path::PathBuf;

fn main() {
    // locate the built plugin artifact
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("plugins");
    path.push("plugin-a");
    path.push("target");
    path.push("debug");
    // Try common build artifact names (dash vs underscore) and platform variations.
    #[cfg(target_os = "windows")]
    {
        let candidates = ["plugin-a.dll", "plugin_a.dll"];
        let mut chosen = None;
        for c in &candidates {
            let mut p = path.clone();
            p.push(c);
            if p.exists() {
                chosen = Some(p);
                break;
            }
        }
        if let Some(p) = chosen {
            path = p;
        } else {
            // fallback to the original dashed name
            path.push("plugin-a.dll");
        }
    }
    #[cfg(target_os = "linux")]
    {
        let candidates = ["libplugin-a.so", "libplugin_a.so"];
        let mut chosen = None;
        for c in &candidates {
            let mut p = path.clone();
            p.push(c);
            if p.exists() {
                chosen = Some(p);
                break;
            }
        }
        if let Some(p) = chosen {
            path = p;
        } else {
            path.push("libplugin-a.so");
        }
    }
    #[cfg(target_os = "macos")]
    {
        let candidates = ["libplugin-a.dylib", "libplugin_a.dylib"];
        let mut chosen = None;
        for c in &candidates {
            let mut p = path.clone();
            p.push(c);
            if p.exists() {
                chosen = Some(p);
                break;
            }
        }
        if let Some(p) = chosen {
            path = p;
        } else {
            path.push("libplugin-a.dylib");
        }
    }

    println!("Loading plugin from {:?}", path);

    match load_greeter_from_lib(&path) {
        Ok((lib, arr_ptr)) => {
            // Keep `lib` in this scope so the DLL stays loaded while we call into it.
            if arr_ptr.is_null() {
                eprintln!("No registrations returned by plugin");
                return;
            }
            let arr = unsafe { &*arr_ptr };
            if arr.count == 0 || arr.registrations.is_null() {
                eprintln!("No registrations returned by plugin");
                return;
            }
            let slice = unsafe { std::slice::from_raw_parts(arr.registrations, arr.count) };
            let reg = unsafe { &*(slice[0] as *const GreeterRegistration) };
            let vtable = unsafe { &*reg.vtable };
            let name_ptr = (vtable.name)(vtable.user_data);
            let cstr = unsafe { CStr::from_ptr(name_ptr) };
            println!("Plugin name: {}", cstr.to_str().unwrap_or("<invalid utf8>"));
            // call greet
            let target = std::ffi::CString::new("world").unwrap();
            (vtable.greet)(vtable.user_data, target.as_ptr());

            // Now call the unload helper which will invoke the plugin's unregister and drop the library.
            unsafe {
                if let Err(e) = plugin_interface::unload_greeter(lib, arr_ptr) {
                    eprintln!("Failed to unload plugin cleanly: {}", e);
                } else {
                    println!("Plugin unloaded cleanly.");
                }
            }
        }
        Err(e) => eprintln!("Failed to load plugin: {}", e),
    }
}
