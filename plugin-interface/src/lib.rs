use libloading::Library;
use std::ffi::c_void;
use std::os::raw::c_char;

// Vtable definition that plugin-annotations macro will generate-compatible vtables for.
#[repr(C)]
pub struct GreeterVTable {
    pub abi_version: u32,
    pub user_data: *mut c_void,
    pub name: extern "C" fn(*mut c_void) -> *const c_char,
    pub greet: extern "C" fn(*mut c_void, *const c_char),
    pub drop: extern "C" fn(*mut c_void),
}

#[repr(C)]
pub struct GreeterRegistration {
    pub name: *const c_char,
    pub vtable: *const GreeterVTable,
}

#[repr(C)]
pub struct RegistrationArray {
    /// Number of registrations in the array.
    pub count: usize,
    /// Type-erased pointer array: *const *const c_void; callers cast entries to the
    /// concrete registration pointer type they expect (for example, `*const GreeterRegistration`).
    pub registrations: *const *const c_void,
    /// Parallel array of pointers to the RegistrationFactory that produced the
    /// corresponding registration entry. This allows precise, deterministic
    /// unmaker calls for each registration.
    pub factories: *const *const RegistrationFactory,
}

/// A small wrapper used with `inventory` so plugins can register their factory functions
/// at link time. Each item holds a function pointer to the plugin's `plugin_register_*`.
/// We store the function pointer as an erased extern "C" function pointer so it can be
/// submitted via `inventory::submit!` without relying on pointer-to-integer casts.
#[repr(C)]
pub struct RegistrationFactory {
    /// Erased factory function pointer: extern "C" fn() -> *const c_void
    pub maker: extern "C" fn() -> *const c_void,
    /// Erased unregister function pointer: extern "C" fn(*const c_void)
    /// that releases a registration previously returned by `maker`.
    pub unmaker: extern "C" fn(*const c_void),
    /// Nul-terminated trait name to allow filtering by trait at runtime.
    pub trait_name: *const c_char,
}

inventory::collect!(RegistrationFactory);
// Raw pointers are inherently fine for static registration; assert thread-safety
unsafe impl Send for RegistrationFactory {}
unsafe impl Sync for RegistrationFactory {}

#[repr(C)]
pub struct PluginMetadata {
    pub name: *const c_char,
    pub abi_version: u32,
    pub vtable: *const c_void,
}

// Example trait to demonstrate prototype
pub trait Greeter {
    fn name(&self) -> &str;
    fn greet(&self, target: &str);
}

mod handle;
mod manager;
pub use handle::{GreeterProxy, PluginHandle};
#[cfg(feature = "watch")]
pub use manager::{ManagerNotification, WatchEvent, WatchNotification, WatchOptions};
pub use manager::{PluginLoadError, PluginManager, PluginUnloadError};

// A tiny loader helper that expects the plugin to export an extern "C" fn
// named `plugin_register_Greeter_v1` returning *const PluginMetadata.
pub fn load_greeter_from_lib(
    path: &std::path::Path,
) -> Result<(Library, *const RegistrationArray), String> {
    let lib = unsafe { Library::new(path) }.map_err(|e| e.to_string())?;
    unsafe {
        // Try the aggregated symbol first
        let all_sym = lib.get::<unsafe extern "C" fn() -> *const RegistrationArray>(
            b"plugin_register_all_Greeter_v1",
        );
        if let Ok(f_all) = all_sym {
            let arr_ptr = f_all();
            if arr_ptr.is_null() {
                return Err("plugin returned null registration array".to_string());
            }
            let arr = &*arr_ptr;
            if arr.count == 0 || arr.registrations.is_null() {
                return Err("plugin registration array empty".to_string());
            }
            return Ok((lib, arr_ptr));
        }

        // Fallback: single registration symbol (erased pointer)
        let symbol: libloading::Symbol<unsafe extern "C" fn() -> *const std::ffi::c_void> = lib
            .get(b"plugin_register_Greeter_v1")
            .map_err(|e| e.to_string())?;
        let reg_ptr = symbol();
        let reg = reg_ptr as *const GreeterRegistration;
        if reg.is_null() {
            Err("plugin returned null registration".to_string())
        } else {
            // Build a host-owned RegistrationArray for the single registration.
            let erased: Vec<*const c_void> = vec![reg as *const c_void];
            let boxed_slice = erased.into_boxed_slice();
            let regs_ptr = Box::into_raw(boxed_slice) as *const *const c_void;
            // No factory pointer available for fallback; set factories to null.
            let arr = Box::new(RegistrationArray {
                count: 1,
                registrations: regs_ptr,
                factories: std::ptr::null(),
            });
            let arr_ptr = Box::into_raw(arr);
            Ok((lib, arr_ptr))
        }
    }
}

/// Call the plugin's unregister function (if present) and then drop the provided Library.
/// Takes ownership of the Library so the plugin can be safely unloaded when this returns.
///
/// # Safety
/// - `arr_ptr` must either be null or point to a `RegistrationArray` previously
///   returned by `load_greeter_from_lib` whose memory ownership is correctly
///   conveyed to this function. If the array was allocated by the host (the
///   fallback single-registration path) this function will free those
///   allocations; if the array was provided by the plugin (and therefore
///   points into plugin-owned memory) the host must not attempt to free them.
/// - The provided `lib` must be the same `Library` instance that produced the
///   registrations and must remain valid for the duration of this call.
/// - No other threads may access the registrations, their vtables, or the
///   library while this function is running.
///
/// Semantics summary:
/// - If `arr_ptr` is null this function simply drops `lib` and returns Ok.
/// - If `arr_ptr.factories` is null the function assumes the host owns the
///   allocations, will attempt to call plugin-provided unregister helpers if
///   present, and will free the host-owned allocations when finished.
/// - If `arr_ptr.factories` is non-null the function prefers the plugin's
///   bulk-unregister helper; otherwise it deterministically invokes each
///   `RegistrationFactory::unmaker` for the corresponding registration.
///
/// The caller is responsible for upholding the invariants above; failure to
/// do so may lead to undefined behavior.
pub unsafe fn unload_greeter(
    lib: Library,
    arr_ptr: *const RegistrationArray,
) -> Result<(), String> {
    if arr_ptr.is_null() {
        drop(lib);
        return Ok(());
    }

    let arr_ref = &*arr_ptr;
    let count = arr_ref.count;
    if count == 0 || arr_ref.registrations.is_null() {
        drop(lib);
        return Ok(());
    }

    let regs_slice = std::slice::from_raw_parts(arr_ref.registrations, count);

    // If factories is null we assume the RegistrationArray was created by the
    // host (fallback single-registration path). The host owns the allocations
    // and is responsible for freeing them after calling any unregister helpers.
    if arr_ref.factories.is_null() {
        // Prefer plugin bulk unregister if present.
        if let Ok(f_all_unreg) = lib.get::<unsafe extern "C" fn(*const RegistrationArray)>(
            b"plugin_unregister_all_Greeter_v1",
        ) {
            f_all_unreg(arr_ptr);
        } else if let Ok(fsym) = lib
            .get::<unsafe extern "C" fn(*const std::ffi::c_void)>(b"plugin_unregister_Greeter_v1")
        {
            for &r in regs_slice.iter() {
                if !r.is_null() {
                    fsym(r);
                }
            }
        }

        // Free host-owned registrations and the RegistrationArray itself.
        let regs_ptr = arr_ref.registrations as *mut *const c_void;
        let _boxed_slice: Box<[*const c_void]> =
            Box::from_raw(core::ptr::slice_from_raw_parts_mut(regs_ptr, count));
        let _ = Box::from_raw(arr_ptr as *mut RegistrationArray);
        drop(lib);
        return Ok(());
    }

    // Plugin-provided RegistrationArray: prefer plugin bulk-unregister helper,
    // otherwise deterministically invoke each registration's factory.unmaker.
    if let Ok(f_all_unreg) = lib
        .get::<unsafe extern "C" fn(*const RegistrationArray)>(b"plugin_unregister_all_Greeter_v1")
    {
        f_all_unreg(arr_ptr);
        drop(lib);
        return Ok(());
    }

    let fac_slice = std::slice::from_raw_parts(arr_ref.factories, count);
    for i in 0..count {
        let r = regs_slice[i];
        if r.is_null() {
            continue;
        }
        let fac_ptr = fac_slice[i];
        if !fac_ptr.is_null() {
            let fac_ref: &RegistrationFactory = &*fac_ptr;
            (fac_ref.unmaker)(r);
        } else if let Ok(fsym) = lib
            .get::<unsafe extern "C" fn(*const std::ffi::c_void)>(b"plugin_unregister_Greeter_v1")
        {
            fsym(r);
        }
    }

    drop(lib);
    Ok(())
}

/// Helper to read the generated versioned unmaker counter for a trait from a
/// loaded plugin `Library`.
///
/// The generated symbol name is `plugin_unmaker_counter_<Trait>_v1` and the
/// symbol is an `extern "C" fn() -> usize` that returns the current counter
/// value (atomic load). This helper constructs the symbol name, looks it up
/// in the provided `lib`, calls it, and returns the value.
pub fn get_unmaker_counter(lib: &Library, trait_name: &str) -> Result<u64, String> {
    // Build null-terminated symbol name expected by libloading::Library::get
    let sym = format!("plugin_unmaker_counter_{}_v1\0", trait_name);
    unsafe {
        let func: libloading::Symbol<unsafe extern "C" fn() -> u64> =
            lib.get(sym.as_bytes()).map_err(|e| e.to_string())?;
        Ok(func())
    }
}

/// Typed identifier for known traits exposed by plugins.
///
/// Prefer passing this enum to host helpers instead of raw strings so callers
/// cannot accidentally misspell trait names at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginTrait {
    Greeter,
}

impl PluginTrait {
    /// Returns the canonical trait name used in generated symbols.
    pub fn as_str(self) -> &'static str {
        match self {
            PluginTrait::Greeter => "Greeter",
        }
    }

    /// Build the C-style null-terminated symbol name bytes expected by
    /// `libloading::Library::get` for the generated unmaker counter getter.
    pub fn symbol_name_bytes(self) -> Vec<u8> {
        format!("plugin_unmaker_counter_{}_v1\0", self.as_str()).into_bytes()
    }
}

/// Typed variant of `get_unmaker_counter` that accepts a `PluginTrait` enum
/// instead of a raw string. This is safer for callers that work with a known
/// set of traits at compile time.
pub fn get_unmaker_counter_for(lib: &Library, trait_id: PluginTrait) -> Result<u64, String> {
    let sym_bytes = trait_id.symbol_name_bytes();
    unsafe {
        let func: libloading::Symbol<unsafe extern "C" fn() -> u64> =
            lib.get(&sym_bytes).map_err(|e| e.to_string())?;
        Ok(func())
    }
}

/// Call a raw unmaker counter getter function pointer and return its value.
/// This is provided to make it easy to unit-test the calling convention and
/// the helper logic without requiring a real dynamic library export.
pub fn call_unmaker_getter_fn(func: unsafe extern "C" fn() -> u64) -> u64 {
    unsafe { func() }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Export a test getter symbol from this test module so we can call
    // `get_unmaker_counter` against the current process library.
    #[no_mangle]
    pub extern "C" fn plugin_unmaker_counter_TestTrait_v1() -> u64 {
        42u64
    }

    #[test]
    fn get_unmaker_counter_calls_local_exported_getter() {
        // Directly call the test getter via the helper to ensure the calling
        // convention and return value are correct.
        let val = call_unmaker_getter_fn(plugin_unmaker_counter_TestTrait_v1);
        assert_eq!(val, 42u64);
    }
}
