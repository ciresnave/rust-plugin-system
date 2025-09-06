use crate::{GreeterRegistration, PluginTrait, RegistrationArray};
use libloading::Library;
use std::ffi::{CStr, CString};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Internal shared data for a loaded library
pub struct LoadedLib {
    pub lib: Library,
    pub arr_ptr: *const RegistrationArray,
    /// Path from which this library was loaded (for manager bookkeeping)
    pub path: std::path::PathBuf,
    // We keep ownership flags: true if the RegistrationArray was created by host
    pub host_owned: bool,
    pub trait_id: PluginTrait,
    pub closed: AtomicBool,
}

impl std::fmt::Debug for LoadedLib {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedLib")
            .field("path", &self.path)
            .field("trait_id", &self.trait_id)
            .field("host_owned", &self.host_owned)
            .field("closed", &self.closed.load(Ordering::SeqCst))
            .finish()
    }
}

impl LoadedLib {
    pub fn new_with_lib(
        lib: Library,
        arr_ptr: *const RegistrationArray,
        trait_id: PluginTrait,
        path: std::path::PathBuf,
    ) -> Self {
        Self {
            lib,
            arr_ptr,
            path,
            host_owned: false,
            trait_id,
            closed: AtomicBool::new(false),
        }
    }

    pub fn new_host_owned(
        lib: Library,
        arr_ptr: *const RegistrationArray,
        trait_id: PluginTrait,
        path: std::path::PathBuf,
    ) -> Self {
        Self {
            lib,
            arr_ptr,
            path,
            host_owned: true,
            trait_id,
            closed: AtomicBool::new(false),
        }
    }
}

/// Opaque handle id type
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PluginId(pub u128);

/// A handle representing a single registration inside a loaded library.
#[derive(Clone, Debug)]
pub struct PluginHandle {
    inner: Arc<LoadedLib>,
    index: usize,
    trait_id: PluginTrait,
    id: PluginId,
}

impl PluginHandle {
    pub fn new(inner: Arc<LoadedLib>, index: usize, trait_id: PluginTrait) -> Self {
        let ptr_val = inner.arr_ptr as usize as u128;
        let id = PluginId((index as u128) ^ ptr_val);
        Self {
            inner,
            index,
            trait_id,
            id,
        }
    }

    pub fn id(&self) -> PluginId {
        self.id
    }

    pub fn as_greeter(&self) -> Option<GreeterProxy> {
        if self.trait_id != PluginTrait::Greeter {
            return None;
        }
        Some(GreeterProxy {
            inner: self.inner.clone(),
            index: self.index,
        })
    }

    /// Close/unload this plugin registration. If we are the last Arc owner
    /// perform unload now and return the plugin unmaker counter if available.
    /// Otherwise set closed and defer unload to the final Drop.
    pub fn close(self) -> Result<Option<u64>, String> {
        let was_closed = self.inner.closed.swap(true, Ordering::SeqCst);
        if was_closed {
            return Ok(None);
        }

        match Arc::try_unwrap(self.inner) {
            Ok(loaded) => unload_loaded_lib(loaded),
            Err(_arc) => Ok(None),
        }
    }
}

pub(crate) fn unload_loaded_lib(mut loaded: LoadedLib) -> Result<Option<u64>, String> {
    let res = perform_unload_mut(&mut loaded);
    loaded.closed.store(true, Ordering::SeqCst);
    res
}

fn perform_unload_mut(loaded: &mut LoadedLib) -> Result<Option<u64>, String> {
    unsafe {
        let lib = &loaded.lib;
        let arr_ptr = loaded.arr_ptr;
        let trait_id = loaded.trait_id;
        if arr_ptr.is_null() {
            return Ok(None);
        }

        let arr_ref = &*arr_ptr;
        let count = arr_ref.count;
        if count == 0 || arr_ref.registrations.is_null() {
            return Ok(None);
        }

        let regs_slice = std::slice::from_raw_parts(arr_ref.registrations, count);

        let unreg_all_sym = format!("plugin_unregister_all_{}_v1\0", trait_id.as_str());
        let unreg_single_sym = format!("plugin_unregister_{}_v1\0", trait_id.as_str());
        let counter_sym = format!("plugin_unmaker_counter_{}_v1\0", trait_id.as_str());

        if arr_ref.factories.is_null() {
            if let Ok(f_all_unreg) =
                lib.get::<unsafe extern "C" fn(*const RegistrationArray)>(unreg_all_sym.as_bytes())
            {
                f_all_unreg(arr_ptr);
            } else if let Ok(fsym) = lib
                .get::<unsafe extern "C" fn(*const std::ffi::c_void)>(unreg_single_sym.as_bytes())
            {
                for &r in regs_slice.iter() {
                    if !r.is_null() {
                        fsym(r);
                    }
                }
            }

            let counter = match lib.get::<unsafe extern "C" fn() -> u64>(counter_sym.as_bytes()) {
                Ok(getter) => Some(getter()),
                Err(_) => None,
            };

            let regs_ptr = arr_ref.registrations as *mut *const std::ffi::c_void;
            let _boxed_slice: Box<[*const std::ffi::c_void]> =
                Box::from_raw(core::ptr::slice_from_raw_parts_mut(regs_ptr, count));
            let _ = Box::from_raw(arr_ptr as *mut RegistrationArray);
            return Ok(counter);
        }

        if let Ok(f_all_unreg) =
            lib.get::<unsafe extern "C" fn(*const RegistrationArray)>(unreg_all_sym.as_bytes())
        {
            f_all_unreg(arr_ptr);
        } else {
            let fac_slice = std::slice::from_raw_parts(arr_ref.factories, count);
            for i in 0..count {
                let r = regs_slice[i];
                if r.is_null() {
                    continue;
                }
                let fac_ptr = fac_slice[i];
                if !fac_ptr.is_null() {
                    let fac_ref: &crate::RegistrationFactory = &*fac_ptr;
                    (fac_ref.unmaker)(r);
                } else if let Ok(fsym) = lib.get::<unsafe extern "C" fn(*const std::ffi::c_void)>(
                    unreg_single_sym.as_bytes(),
                ) {
                    fsym(r);
                }
            }
        }

        let counter = match lib.get::<unsafe extern "C" fn() -> u64>(counter_sym.as_bytes()) {
            Ok(getter) => Some(getter()),
            Err(_) => None,
        };
        Ok(counter)
    }
}

impl Drop for LoadedLib {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::SeqCst) {
            let _ = perform_unload_mut(self);
            self.closed.store(true, Ordering::SeqCst);
        }
    }
}

/// Safe proxy for Greeter trait that hides vtable access.
#[derive(Clone, Debug)]
pub struct GreeterProxy {
    inner: Arc<LoadedLib>,
    index: usize,
}

impl GreeterProxy {
    pub fn name(&self) -> String {
        unsafe {
            let arr = &*self.inner.arr_ptr;
            let regs = std::slice::from_raw_parts(arr.registrations, arr.count);
            let reg = &*(regs[self.index] as *const GreeterRegistration);
            let v = &*reg.vtable;
            let c = (v.name)(v.user_data);
            CStr::from_ptr(c).to_string_lossy().into_owned()
        }
    }

    pub fn greet(&self, target: &str) {
        let c_target = CString::new(target).expect("target contains null");
        unsafe {
            let arr = &*self.inner.arr_ptr;
            let regs = std::slice::from_raw_parts(arr.registrations, arr.count);
            let reg = &*(regs[self.index] as *const GreeterRegistration);
            let v = &*reg.vtable;
            (v.greet)(v.user_data, c_target.as_ptr());
        }
    }
}
