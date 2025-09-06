use crate::{PluginTrait, RegistrationArray};
use libloading::Library;
use std::collections::HashSet;
use std::path::Path;
#[cfg(feature = "watch")]
use std::path::PathBuf;
#[cfg(feature = "watch")]
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Weak};
#[cfg(feature = "watch")]
use std::thread;
#[cfg(feature = "watch")]
use std::time::Duration;

use crate::handle::{unload_loaded_lib, LoadedLib, PluginHandle};

/// Errors when loading plugins
#[derive(Debug)]
pub enum PluginLoadError {
    Io(std::io::Error),
    Lib(String),
    NoRegistrations,
}

/// Errors when unloading
#[derive(Debug)]
pub enum PluginUnloadError {
    Lib(String),
}

pub struct PluginManager {
    // Weak refs to loaded libs; handles own the strong Arcs so unload can occur
    libs: Vec<Weak<LoadedLib>>,
    // track file paths we've already loaded to avoid duplicates
    loaded_paths: HashSet<std::path::PathBuf>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    /// Attempt to unload the library previously loaded from `path`.
    /// If the manager is the only owner (strong_count == 1) this will
    /// perform the unload immediately and return the plugin unmaker counter
    /// if available. If there are other owners the manager will mark the
    /// LoadedLib as closed so the final owner will perform the unload on Drop
    /// and return None.
    pub fn unload_by_path(&mut self, path: &std::path::Path) -> Result<Option<u64>, String> {
        let mut i = 0usize;
        while i < self.libs.len() {
            if let Some(strong) = self.libs[i].upgrade() {
                // compare path
                if strong.path == path {
                    // if manager is the only owner, try to take it and unload now
                    if Arc::strong_count(&strong) == 1 {
                        // remove this weak entry
                        self.libs.remove(i);
                        self.loaded_paths.remove(path);
                        // Try to consume the Arc
                        match Arc::try_unwrap(strong) {
                            Ok(loaded) => return unload_loaded_lib(loaded),
                            Err(_) => return Ok(None),
                        }
                    } else {
                        // mark closed so the final owner will run unload on Drop
                        strong
                            .closed
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        self.loaded_paths.remove(path);
                        // keep weak entry around; advance
                        return Ok(None);
                    }
                } else {
                    i += 1;
                }
            } else {
                // dead weak ref; remove
                self.libs.remove(i);
            }
        }
        Ok(None)
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            libs: Vec::new(),
            loaded_paths: HashSet::new(),
        }
    }

    #[allow(clippy::arc_with_non_send_sync)]
    pub fn load_plugins(
        &mut self,
        dir: &Path,
        trait_id: PluginTrait,
    ) -> Result<Vec<PluginHandle>, PluginLoadError> {
        let mut handles = Vec::new();
        let read_dir = dir.read_dir().map_err(PluginLoadError::Io)?;
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !is_dynamic_library(path.as_path()) {
                continue;
            }

            if self.loaded_paths.contains(&path) {
                continue;
            }

            // Try to open the library
            let lib =
                unsafe { Library::new(&path) }.map_err(|e| PluginLoadError::Lib(e.to_string()))?;

            // Build symbol name for aggregated register_all
            let sym = format!("plugin_register_all_{}_v1\0", trait_id.as_str());
            unsafe {
                if let Ok(f_all) =
                    lib.get::<unsafe extern "C" fn() -> *const RegistrationArray>(sym.as_bytes())
                {
                    let arr_ptr = f_all();
                    if arr_ptr.is_null() {
                        continue;
                    }
                    let loaded = Arc::new(LoadedLib::new_with_lib(
                        lib,
                        arr_ptr,
                        trait_id,
                        path.clone(),
                    ));
                    let count = (&*arr_ptr).count;
                    for idx in 0..count {
                        let h = PluginHandle::new(loaded.clone(), idx, trait_id);
                        handles.push(h);
                    }
                    self.libs.push(Arc::downgrade(&loaded));
                    self.loaded_paths.insert(path.clone());
                    continue;
                }

                // Fallback: single registration symbol
                let single_sym = format!("plugin_register_{}_v1\0", trait_id.as_str());
                if let Ok(f_single) = lib
                    .get::<unsafe extern "C" fn() -> *const std::ffi::c_void>(single_sym.as_bytes())
                {
                    let reg_ptr = f_single();
                    if reg_ptr.is_null() {
                        continue;
                    }
                    // Build a host-owned RegistrationArray for the single registration.
                    let erased: Vec<*const std::ffi::c_void> = vec![reg_ptr];
                    let boxed_slice = erased.into_boxed_slice();
                    let regs_ptr = Box::into_raw(boxed_slice) as *const *const std::ffi::c_void;
                    let arr = Box::new(RegistrationArray {
                        count: 1,
                        registrations: regs_ptr,
                        factories: std::ptr::null(),
                    });
                    let arr_ptr = Box::into_raw(arr);
                    let loaded = Arc::new(LoadedLib::new_host_owned(
                        lib,
                        arr_ptr,
                        trait_id,
                        path.clone(),
                    ));
                    let h = PluginHandle::new(loaded.clone(), 0, trait_id);
                    handles.push(h);
                    self.libs.push(Arc::downgrade(&loaded));
                    self.loaded_paths.insert(path.clone());
                    continue;
                }
            }
        }

        if handles.is_empty() {
            return Err(PluginLoadError::NoRegistrations);
        }

        Ok(handles)
    }
}

#[cfg(feature = "watch")]
/// Simple event type emitted by the watcher when a new library file appears
#[derive(Debug, Clone)]
pub enum PluginEvent {
    NewPlugin(PathBuf),
}

#[cfg(feature = "watch")]
/// Event delivered to the synchronous watcher callback. Either raw
/// PluginHandle values or typed GreeterProxy wrappers (when available)
/// are delivered depending on `WatchOptions`.
#[derive(Debug)]
pub enum WatchEvent {
    Handles(Vec<PluginHandle>, Vec<PathBuf>),
    Proxies(Vec<crate::GreeterProxy>, Vec<PathBuf>),
}

#[cfg(feature = "watch")]
impl PluginManager {
    /// Watch `dir` for new dynamic libraries exposing `trait_id` and emit
    /// a `PluginEvent::NewPlugin(PathBuf)` for each new file found. This is
    /// implemented with a simple polling loop to avoid adding heavy
    /// platform-specific watcher dependencies. The polling loop runs in a
    /// background thread and returns a Receiver to receive events; caller
    /// should drop the Receiver to stop listening (the thread will continue
    /// until the process exits).
    pub fn watch_plugins(&mut self, dir: PathBuf, _trait_id: PluginTrait) -> Receiver<PluginEvent> {
        let (tx, rx) = mpsc::channel();

        // build a thread-local seen set to avoid notifying for files that
        // already exist when the watcher starts
        let mut seen: HashSet<PathBuf> = HashSet::new();
        if let Ok(read_dir) = dir.read_dir() {
            for e in read_dir.flatten() {
                let p = e.path();
                if is_dynamic_library(p.as_path()) {
                    seen.insert(p);
                }
            }
        }

        let tx_clone = tx.clone();
        thread::spawn(move || {
            let mut seen = seen;
            loop {
                if let Ok(read_dir) = dir.read_dir() {
                    for e in read_dir.flatten() {
                        let p = e.path();
                        if !is_dynamic_library(p.as_path()) {
                            continue;
                        }
                        if seen.contains(&p) {
                            continue;
                        }
                        seen.insert(p.clone());
                        // try to send for new files
                        let _ = tx_clone.send(PluginEvent::NewPlugin(p.clone()));
                    }
                }
                thread::sleep(Duration::from_millis(500));
            }
        });

        rx
    }

    // ...existing code...

    /// Watch `dir` and call `load_plugins` internally when new dynamic
    /// libraries appear. The provided callback is invoked on the same thread
    /// that called this method; it receives a Vec of loaded `PluginHandle`s
    /// (may be empty on error or when `auto_load` is false) and a Vec of the
    /// file paths that triggered the event. Return `true` from the callback
    /// to continue watching, or `false` to stop.
    pub fn watch_and_load_blocking<F>(
        &mut self,
        dir: PathBuf,
        trait_id: PluginTrait,
        opts: WatchOptions,
        mut callback: F,
    ) where
        F: FnMut(WatchEvent) -> bool,
    {
        use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

        // initial seen set
        let mut seen: HashSet<PathBuf> = HashSet::new();
        if let Ok(read_dir) = dir.read_dir() {
            for e in read_dir.flatten() {
                let p = e.path();
                if is_dynamic_library(p.as_path()) {
                    seen.insert(p);
                }
            }
        }

        let (raw_tx, raw_rx) = mpsc::channel();

        let mut watcher: RecommendedWatcher = match RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                let _ = raw_tx.send(res);
            },
            notify::Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("watcher error: {}", e);
                return;
            }
        };

        let mode = if opts.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        if let Err(e) = watcher.watch(&dir, mode) {
            eprintln!("failed to watch dir {:?}: {}", dir, e);
            return;
        }

        let mut debounce_map: std::collections::HashMap<PathBuf, std::time::Instant> =
            std::collections::HashMap::new();

        loop {
            match raw_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok(event)) => {
                    // handle create/modify as potential new plugin candidates
                    if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                        for path in event.paths.iter() {
                            if !is_dynamic_library(path) {
                                continue;
                            }
                            if seen.contains(path) {
                                continue;
                            }
                            debounce_map.insert(path.clone(), std::time::Instant::now());
                        }
                    }

                    // handle remove events: attempt to unload if requested and notify via callback
                    if matches!(event.kind, EventKind::Remove(_)) {
                        for path in event.paths.iter() {
                            if !is_dynamic_library(path) {
                                continue;
                            }
                            // if requested, attempt to unload now on this same thread
                            if opts.auto_unload {
                                let _ = self.unload_by_path(path);
                            }
                            // inform callback of removal; send empty Handles or Proxies
                            if opts.emit_proxies && trait_id == PluginTrait::Greeter {
                                let cont =
                                    callback(WatchEvent::Proxies(Vec::new(), vec![path.clone()]));
                                if !cont {
                                    return;
                                }
                            } else {
                                let cont =
                                    callback(WatchEvent::Handles(Vec::new(), vec![path.clone()]));
                                if !cont {
                                    return;
                                }
                            }
                        }
                    }
                }
                Ok(Err(_)) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let now = std::time::Instant::now();
                    let mut ready: Vec<PathBuf> = Vec::new();
                    let debounce_ms = opts.debounce_ms;
                    debounce_map.retain(|p, t| {
                        if now.duration_since(*t).as_millis() as u64 >= debounce_ms {
                            ready.push(p.clone());
                            false
                        } else {
                            true
                        }
                    });

                    if !ready.is_empty() {
                        // mark seen and either auto-load or just report paths
                        for p in ready.iter() {
                            seen.insert(p.clone());
                        }

                        if opts.auto_load {
                            // attempt to load plugins from dir; ignore errors and
                            // pass empty handles on error.
                            match self.load_plugins(&dir, trait_id) {
                                Ok(handles) => {
                                    if opts.emit_proxies && trait_id == PluginTrait::Greeter {
                                        let proxies: Vec<crate::GreeterProxy> =
                                            handles.iter().filter_map(|h| h.as_greeter()).collect();
                                        let cont =
                                            callback(WatchEvent::Proxies(proxies, ready.clone()));
                                        if !cont {
                                            break;
                                        }
                                    } else {
                                        let cont =
                                            callback(WatchEvent::Handles(handles, ready.clone()));
                                        if !cont {
                                            break;
                                        }
                                    }
                                }
                                Err(_) => {
                                    if opts.emit_proxies && trait_id == PluginTrait::Greeter {
                                        let cont = callback(WatchEvent::Proxies(
                                            Vec::new(),
                                            ready.clone(),
                                        ));
                                        if !cont {
                                            break;
                                        }
                                    } else {
                                        let cont = callback(WatchEvent::Handles(
                                            Vec::new(),
                                            ready.clone(),
                                        ));
                                        if !cont {
                                            break;
                                        }
                                    }
                                }
                            }
                        } else {
                            if opts.emit_proxies && trait_id == PluginTrait::Greeter {
                                let cont = callback(WatchEvent::Proxies(Vec::new(), ready.clone()));
                                if !cont {
                                    break;
                                }
                            } else {
                                let cont = callback(WatchEvent::Handles(Vec::new(), ready.clone()));
                                if !cont {
                                    break;
                                }
                            }
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    }
}

#[cfg(feature = "watch")]
/// Notifications emitted by the background watcher thread. These are intentionally
/// conservative (PathBufs and unload notifications) because richer types like
/// PluginHandle or GreeterProxy may not be Send/Sync and therefore cannot be
/// safely transmitted across thread boundaries.
#[derive(Debug)]
pub enum WatchNotification {
    /// One or more discovered paths that passed the debounce window.
    Paths(Vec<PathBuf>),
    /// A library path was removed (or otherwise considered removed) and the
    /// watcher observed it; the optional counter is the result of attempting
    /// to deterministically unload the library (manager must perform unload).
    Unloaded { path: PathBuf, counter: Option<u64> },
    /// Error string from watcher or internal failure.
    Error(String),
}

#[cfg(feature = "watch")]
impl PluginManager {
    /// Start watching `dir` in a background thread for filesystem events and
    /// return a Receiver of conservative notifications plus the JoinHandle for
    /// the spawned thread. The background watcher does NOT attempt to call
    /// `load_plugins` or `unload_by_path` on the manager because the manager
    /// may not be Send/Sync; instead it emits path-level notifications which
    /// the caller can handle on the thread owning the manager (for example by
    /// calling `load_plugins` or `unload_by_path`). This avoids sending
    /// non-Send plugin handles across threads.
    pub fn start_watch_background(
        &mut self,
        dir: PathBuf,
        opts: WatchOptions,
    ) -> (
        Receiver<WatchNotification>,
        std::sync::mpsc::Sender<()>,
        std::thread::JoinHandle<()>,
    ) {
        let (tx, rx) = mpsc::channel::<WatchNotification>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();

        // build a thread-local seen set to avoid notifying for files that
        // already exist when the watcher starts
        let mut seen: HashSet<PathBuf> = HashSet::new();
        if let Ok(read_dir) = dir.read_dir() {
            for e in read_dir.flatten() {
                let p = e.path();
                if is_dynamic_library(&p) {
                    seen.insert(p);
                }
            }
        }

        // Spawn the watcher thread. The thread only sends conservative
        // notifications back to the caller via the channel.
        let thread_dir = dir.clone();
        let handle = thread::spawn(move || {
            use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

            let (raw_tx, raw_rx) = mpsc::channel();
            let mut watcher: RecommendedWatcher = match RecommendedWatcher::new(
                move |res: Result<notify::Event, notify::Error>| {
                    let _ = raw_tx.send(res);
                },
                notify::Config::default(),
            ) {
                Ok(w) => w,
                Err(e) => {
                    let _ = tx.send(WatchNotification::Error(format!(
                        "failed to create watcher: {}",
                        e
                    )));
                    return;
                }
            };

            let mode = if opts.recursive {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };

            if let Err(e) = watcher.watch(&thread_dir, mode) {
                let _ = tx.send(WatchNotification::Error(format!(
                    "failed to watch dir {:?}: {}",
                    thread_dir, e
                )));
                return;
            }

            let mut debounce_map: std::collections::HashMap<PathBuf, std::time::Instant> =
                std::collections::HashMap::new();

            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                match raw_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(event)) => {
                        if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                            for path in event.paths.iter() {
                                if !is_dynamic_library(path.as_path()) {
                                    continue;
                                }
                                if seen.contains(path) {
                                    continue;
                                }
                                debounce_map.insert(path.clone(), std::time::Instant::now());
                            }
                        }

                        if matches!(event.kind, EventKind::Remove(_)) {
                            for path in event.paths.iter() {
                                if !is_dynamic_library(path.as_path()) {
                                    continue;
                                }
                                // report removal to caller; caller may call
                                // `unload_by_path` on the manager if desired.
                                let _ = tx.send(WatchNotification::Unloaded {
                                    path: path.clone(),
                                    counter: None,
                                });
                            }
                        }
                    }
                    Ok(Err(_)) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        let now = std::time::Instant::now();
                        let mut ready: Vec<PathBuf> = Vec::new();
                        let debounce_ms = opts.debounce_ms;
                        debounce_map.retain(|p, t| {
                            if now.duration_since(*t).as_millis() as u64 >= debounce_ms {
                                ready.push(p.clone());
                                false
                            } else {
                                true
                            }
                        });

                        if !ready.is_empty() {
                            for p in ready.iter() {
                                seen.insert(p.clone());
                            }
                            let _ = tx.send(WatchNotification::Paths(ready));
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        (rx, stop_tx, handle)
    }
}

#[cfg(feature = "watch")]
/// Notifications emitted by manager when it processes watch events.
#[derive(Debug)]
pub enum ManagerNotification {
    Event(WatchEvent),
    Unloaded { path: PathBuf, counter: Option<u64> },
    Error(String),
}

#[cfg(feature = "watch")]
impl PluginManager {
    /// Process watch notifications produced by `start_watch_background`.
    /// This method runs on the caller's thread and calls `load_plugins` and
    /// `unload_by_path` on the manager as events arrive. The provided
    /// callback is invoked with `ManagerNotification` for each manager action;
    /// return false from the callback to stop processing and return.
    pub fn process_watch_notifications_blocking<F>(
        &mut self,
        dir: &Path,
        rx: Receiver<WatchNotification>,
        trait_id: PluginTrait,
        opts: WatchOptions,
        mut callback: F,
    ) where
        F: FnMut(ManagerNotification) -> bool,
    {
        loop {
            match rx.recv() {
                Ok(WatchNotification::Paths(paths)) => {
                    if opts.auto_load {
                        match self.load_plugins(dir, trait_id) {
                            Ok(handles) => {
                                if opts.emit_proxies && trait_id == PluginTrait::Greeter {
                                    let proxies: Vec<crate::GreeterProxy> =
                                        handles.iter().filter_map(|h| h.as_greeter()).collect();
                                    if !callback(ManagerNotification::Event(WatchEvent::Proxies(
                                        proxies,
                                        paths.clone(),
                                    ))) {
                                        return;
                                    }
                                } else if !callback(ManagerNotification::Event(
                                    WatchEvent::Handles(handles, paths.clone()),
                                )) {
                                    return;
                                }
                            }
                            Err(e) => {
                                if !callback(ManagerNotification::Error(format!(
                                    "load error: {:?}",
                                    e
                                ))) {
                                    return;
                                }
                            }
                        }
                    } else {
                        // Auto-load disabled: just notify empty events
                        if opts.emit_proxies && trait_id == PluginTrait::Greeter {
                            if !callback(ManagerNotification::Event(WatchEvent::Proxies(
                                Vec::new(),
                                paths.clone(),
                            ))) {
                                return;
                            }
                        } else if !callback(ManagerNotification::Event(WatchEvent::Handles(
                            Vec::new(),
                            paths.clone(),
                        ))) {
                            return;
                        }
                    }
                }
                Ok(WatchNotification::Unloaded { path, .. }) => {
                    // manager performs unload when requested
                    if opts.auto_unload {
                        match self.unload_by_path(&path) {
                            Ok(counter) => {
                                if !callback(ManagerNotification::Unloaded {
                                    path: path.clone(),
                                    counter,
                                }) {
                                    return;
                                }
                            }
                            Err(e) => {
                                if !callback(ManagerNotification::Error(e)) {
                                    return;
                                }
                            }
                        }
                    } else if !callback(ManagerNotification::Unloaded {
                        path: path.clone(),
                        counter: None,
                    }) {
                        return;
                    }
                }
                Ok(WatchNotification::Error(e)) => {
                    if !callback(ManagerNotification::Error(e)) {
                        return;
                    }
                }
                Err(_) => break,
            }
        }
    }
}

fn is_dynamic_library(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        #[cfg(target_os = "windows")]
        return ext.eq_ignore_ascii_case("dll");
        #[cfg(target_os = "macos")]
        return ext.eq_ignore_ascii_case("dylib");
        #[cfg(all(unix, not(target_os = "macos")))]
        return ext.eq_ignore_ascii_case("so");
    }
    false
}

#[cfg(feature = "watch")]
/// Options to configure watching behavior for `watch_and_load_blocking`.
#[derive(Clone)]
pub struct WatchOptions {
    /// Debounce window in milliseconds to coalesce rapid events.
    pub debounce_ms: u64,
    /// Whether to watch directories recursively.
    pub recursive: bool,
    /// If true, call `load_plugins` internally and send PluginHandle values
    /// to the callback; if false, the callback will receive an empty
    /// handles vec and the discovered paths.
    pub auto_load: bool,
    /// If true, attempt to automatically unload plugins when files are
    /// removed or updated. The manager will call `unload_by_path` on remove
    /// events if enabled.
    pub auto_unload: bool,
    /// If true, the watcher will prefer emitting typed proxies (where
    /// possible) instead of raw PluginHandle values when calling the
    /// synchronous callback. Note: proxies may not be Send/Sync and are
    /// therefore not used in the background watcher API.
    pub emit_proxies: bool,
}

#[cfg(feature = "watch")]
impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            debounce_ms: 300,
            recursive: false,
            auto_load: true,
            auto_unload: false,
            emit_proxies: false,
        }
    }
}
