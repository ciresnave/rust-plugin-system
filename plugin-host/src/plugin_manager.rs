use libloading::Symbol;
use plugin_interface::GreeterRegistration;
use std::collections::HashMap;
use std::path::Path;

#[allow(dead_code)]
pub struct PluginEntry {
    pub lib: libloading::Library,
    pub arr_ptr: *const plugin_interface::RegistrationArray,
}

pub struct PluginManager {
    plugins: Vec<PluginEntry>,
    plugin_names: HashMap<usize, String>,
}

impl PluginManager {
    pub fn new() -> Self {
        PluginManager {
            plugins: Vec::new(),
            plugin_names: HashMap::new(),
        }
    }

    pub fn load_plugin<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, String> {
        // Use the shared loader which returns (Library, Vec<registration_ptr>)
        let (lib, arr_ptr) = plugin_interface::load_greeter_from_lib(path.as_ref())?;
        let index = self.plugins.len();
        self.plugins.push(PluginEntry { lib, arr_ptr });
        self.plugin_names.insert(index, format!("Plugin {}", index));
        Ok(index)
    }

    pub fn call_plugin_function(
        &self,
        plugin_index: usize,
        function_name: &str,
    ) -> Result<(), String> {
        if plugin_index >= self.plugins.len() {
            return Err("Plugin index out of bounds".to_string());
        }

        unsafe {
            let entry = &self.plugins[plugin_index];
            let func: Symbol<unsafe extern "C" fn()> = entry
                .lib
                .get(function_name.as_bytes())
                .map_err(|e| e.to_string())?;
            func();
            Ok(())
        }
    }

    /// Call the Greeter.greet method for the first registration of the plugin.
    #[allow(dead_code)]
    pub fn call_greet(&self, plugin_index: usize, target: &str) -> Result<(), String> {
        if plugin_index >= self.plugins.len() {
            return Err("Plugin index out of bounds".to_string());
        }

        let entry = &self.plugins[plugin_index];
        if entry.arr_ptr.is_null() {
            return Err("No registrations available for plugin".to_string());
        }

        unsafe {
            let arr = &*entry.arr_ptr;
            if arr.count == 0 || arr.registrations.is_null() {
                return Err("No registrations available in array".to_string());
            }
            let slice = std::slice::from_raw_parts(arr.registrations, arr.count);
            let reg = &*(slice[0] as *const GreeterRegistration);
            let vtable = &*reg.vtable;
            // call greet
            let c_target = std::ffi::CString::new(target).map_err(|e| e.to_string())?;
            (vtable.greet)(vtable.user_data, c_target.as_ptr());
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn unload_plugin(&mut self, plugin_index: usize) -> Result<(), String> {
        if plugin_index >= self.plugins.len() {
            return Err("Plugin index out of bounds".to_string());
        }

        // Remove the entry and call the shared unload helper which will call unregister and drop the Library.
        let entry = self.plugins.swap_remove(plugin_index);
        unsafe { plugin_interface::unload_greeter(entry.lib, entry.arr_ptr) }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        PluginManager::new()
    }
}
