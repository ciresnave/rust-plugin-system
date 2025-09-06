use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident, ImplItem, ItemImpl, ItemTrait, ReturnType, TraitItem, Type};

/// `#[plugin_interface]` reads a trait and emits a repr(C) vtable+registration and a small
/// loader helper (prototype). It supports trait methods that take &self and either zero or one
/// &str parameter, returning () or &str. This is intentionally narrow for the prototype.
#[proc_macro_attribute]
pub fn plugin_interface(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemTrait);

    let trait_ident = &input.ident;
    let trait_name = trait_ident.to_string();
    let vtable_ident = Ident::new(
        &format!("{}VTable", trait_name),
        proc_macro2::Span::call_site(),
    );
    let registration_ident = Ident::new(
        &format!("{}Registration", trait_name),
        proc_macro2::Span::call_site(),
    );
    let loader_ident = Ident::new(
        &format!("load_{}_from_lib", trait_name.to_lowercase()),
        proc_macro2::Span::call_site(),
    );
    let register_symbol = format!("plugin_register_{}_v1", trait_name);
    let register_lit = proc_macro2::Literal::byte_string(register_symbol.as_bytes());

    // Collect simple method shapes
    let mut method_fields = Vec::new();
    for item in input.items.iter() {
        if let TraitItem::Fn(m) = item {
            let sig = &m.sig;
            let name = sig.ident.to_string();

            let mut has_str_arg = false;
            if sig.inputs.len() > 1 {
                has_str_arg = true;
            }
            let ret_is_str = match &sig.output {
                ReturnType::Type(_, ty) => {
                    let s = quote! { #ty }.to_string();
                    s.contains("str")
                }
                _ => false,
            };

            let field_ident = Ident::new(&name, proc_macro2::Span::call_site());
            let field_ty = if has_str_arg && ret_is_str {
                quote! { extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char) -> *const std::os::raw::c_char }
            } else if has_str_arg {
                quote! { extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char) }
            } else if ret_is_str {
                quote! { extern "C" fn(*mut std::ffi::c_void) -> *const std::os::raw::c_char }
            } else {
                quote! { extern "C" fn(*mut std::ffi::c_void) }
            };

            method_fields.push(quote! { pub #field_ident: #field_ty });
        }
    }

    let generated = quote! {
        #input

        #[repr(C)]
        pub struct #vtable_ident {
            pub abi_version: u32,
            pub user_data: *mut std::ffi::c_void,
            #(#method_fields,)*
            pub drop: extern "C" fn(*mut std::ffi::c_void),
        }

        #[repr(C)]
        pub struct #registration_ident {
            pub name: *const std::os::raw::c_char,
            pub vtable: *const #vtable_ident,
        }

        /// Prototype loader: opens the library and looks up the plugin_register_{Trait}_v1 symbol.
        pub fn #loader_ident(path: &std::path::Path) -> Result<*const #registration_ident, String> {
            let lib = unsafe { libloading::Library::new(path) }.map_err(|e| e.to_string())?;
            unsafe {
                let symbol: libloading::Symbol<unsafe extern "C" fn() -> *const #registration_ident> =
                    lib.get(#register_lit).map_err(|e| e.to_string())?;
                let reg = symbol();
                if reg.is_null() {
                    Err("plugin returned null registration".to_string())
                } else {
                    Ok(reg)
                }
            }
        }
    };

    TokenStream::from(generated)
}

/// `#[plugin_impl(TraitName)]` applied to `impl TraitName for Type` generates C wrappers for
/// the trait methods, a register function that returns a pointer to a heap-allocated
/// registration struct, and an unregister function that frees the heap allocations.
#[proc_macro_attribute]
pub fn plugin_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);

    let trait_path = if !attr.is_empty() {
        Some(parse_macro_input!(attr as syn::Path))
    } else {
        None
    };

    let trait_ident = trait_path
        .as_ref()
        .and_then(|p| p.segments.last())
        .map(|s| s.ident.to_string())
        .unwrap_or_else(|| "Greeter".to_string());

    // prepare a nul-terminated byte string literal for the trait name
    let mut trait_name_bytes = trait_ident.as_bytes().to_vec();
    trait_name_bytes.push(0);
    let trait_name_lit = proc_macro2::Literal::byte_string(&trait_name_bytes);

    // self type
    let self_ty: &Type = &input.self_ty;

    let ty_tokens = quote! { #self_ty };
    let ty_ident_string = ty_tokens.to_string();
    // Build a safe identifier by replacing module separators with '_' and
    // converting generic angle brackets into underscores. Remove spaces.
    let tmp = ty_ident_string.replace("::", "_");
    let safe_name: String = tmp
        .chars()
        .filter_map(|c| match c {
            '<' | '>' => Some('_'),
            ' ' => None,
            other => Some(other),
        })
        .collect();

    // collect methods
    let mut methods: Vec<(String, bool, bool)> = Vec::new();
    for item in input.items.iter() {
        if let ImplItem::Fn(m) = item {
            let sig = &m.sig;
            let name = sig.ident.to_string();
            let mut has_str_arg = false;
            if sig.inputs.len() > 1 {
                has_str_arg = true;
            }
            let ret_is_str = match &sig.output {
                ReturnType::Type(_, ty) => {
                    let s = quote! { #ty }.to_string();
                    s.contains("str")
                }
                _ => false,
            };
            methods.push((name, has_str_arg, ret_is_str));
        }
    }

    // build wrappers and vtable fields
    let mut wrapper_fns = Vec::new();
    let mut vtable_inits = Vec::new();
    let mut vtable_fields = Vec::new();
    for (name, has_str_arg, ret_is_str) in &methods {
        let wrapper_ident = Ident::new(
            &format!("{}_{}_wrapper", safe_name, name),
            proc_macro2::Span::call_site(),
        );
        let field_ident = Ident::new(name.as_str(), proc_macro2::Span::call_site());

        let wrapper = if *has_str_arg && *ret_is_str {
            quote! {
                #[no_mangle]
                pub extern "C" fn #wrapper_ident(user_data: *mut std::ffi::c_void, arg: *const std::os::raw::c_char) -> *const std::os::raw::c_char {
                    let instance = unsafe { &*(user_data as *const #self_ty) };
                    let cstr = unsafe { std::ffi::CStr::from_ptr(arg) };
                    let arg_str = cstr.to_str().unwrap_or("");
                    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        instance.#field_ident(arg_str)
                    }));
                    match res {
                        Ok(s) => std::ffi::CString::new(s).unwrap().into_raw() as *const std::os::raw::c_char,
                        Err(_) => std::ptr::null(),
                    }
                }
            }
        } else if *has_str_arg {
            quote! {
                #[no_mangle]
                pub extern "C" fn #wrapper_ident(user_data: *mut std::ffi::c_void, arg: *const std::os::raw::c_char) {
                    let instance = unsafe { &*(user_data as *const #self_ty) };
                    let cstr = unsafe { std::ffi::CStr::from_ptr(arg) };
                    let arg_str = cstr.to_str().unwrap_or("");
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        instance.#field_ident(arg_str);
                    }));
                }
            }
        } else if *ret_is_str {
            quote! {
                #[no_mangle]
                pub extern "C" fn #wrapper_ident(user_data: *mut std::ffi::c_void) -> *const std::os::raw::c_char {
                    let instance = unsafe { &*(user_data as *const #self_ty) };
                    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        instance.#field_ident()
                    }));
                    match res {
                        Ok(s) => std::ffi::CString::new(s).unwrap().into_raw() as *const std::os::raw::c_char,
                        Err(_) => std::ptr::null(),
                    }
                }
            }
        } else {
            quote! {
                #[no_mangle]
                pub extern "C" fn #wrapper_ident(user_data: *mut std::ffi::c_void) {
                    let instance = unsafe { &*(user_data as *const #self_ty) };
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        instance.#field_ident();
                    }));
                }
            }
        };

        let field_ty = if *has_str_arg && *ret_is_str {
            quote! { extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char) -> *const std::os::raw::c_char }
        } else if *has_str_arg {
            quote! { extern "C" fn(*mut std::ffi::c_void, *const std::os::raw::c_char) }
        } else if *ret_is_str {
            quote! { extern "C" fn(*mut std::ffi::c_void) -> *const std::os::raw::c_char }
        } else {
            quote! { extern "C" fn(*mut std::ffi::c_void) }
        };

        wrapper_fns.push(wrapper);
        vtable_fields.push(quote! { pub #field_ident: #field_ty });
        vtable_inits.push(quote! { #field_ident: #wrapper_ident as #field_ty });
    }

    let trait_vtable_ident = Ident::new(
        &format!("{}VTable", trait_ident),
        proc_macro2::Span::call_site(),
    );
    let trait_registration_ident = Ident::new(
        &format!("{}Registration", trait_ident),
        proc_macro2::Span::call_site(),
    );
    // Make per-impl symbol names unique by including the implementing type name
    let register_symbol = format!("plugin_register_{}_{}_v1", trait_ident, safe_name);
    let register_ident = Ident::new(&register_symbol, proc_macro2::Span::call_site());
    let unregister_symbol = format!("plugin_unregister_{}_{}_v1", trait_ident, safe_name);
    let unregister_ident = Ident::new(&unregister_symbol, proc_macro2::Span::call_site());
    // We will submit a `plugin_interface::RegistrationFactory` instance which
    // contains an erased function pointer and the trait name. The host-side
    // aggregation helpers will filter by trait name.

    // final expansion
    let expanded = quote! {
        #input

        #(#wrapper_fns)*

    #[no_mangle]
    pub extern "C" fn #register_ident() -> *const std::ffi::c_void {
            unsafe {
                let boxed: Box<#self_ty> = Box::new(<#self_ty>::default());
                let user_ptr = Box::into_raw(boxed) as *mut std::ffi::c_void;

                extern "C" fn drop_trampoline(u: *mut std::ffi::c_void) {
                    if u.is_null() { return; }
                    unsafe {
                        let _boxed: Box<#self_ty> = Box::from_raw(u as *mut #self_ty);
                    }
                }

                let vtable = Box::new(plugin_interface::#trait_vtable_ident {
                    abi_version: 1,
                    user_data: user_ptr,
                    #(#vtable_inits,)*
                    drop: drop_trampoline,
                });
                let vtable_ptr = Box::into_raw(vtable);

                let reg = Box::new(plugin_interface::#trait_registration_ident { name: std::ptr::null(), vtable: vtable_ptr });
                Box::into_raw(reg) as *const std::ffi::c_void
            }
        }

    #[no_mangle]
    pub extern "C" fn #unregister_ident(reg_ptr: *const std::ffi::c_void) {
            if reg_ptr.is_null() { return; }
            unsafe {
                let reg_box: Box<plugin_interface::#trait_registration_ident> = Box::from_raw(reg_ptr as *mut _);
                let vtable_ptr = reg_box.vtable as *mut plugin_interface::#trait_vtable_ident;

                // In-process test hook: increment the per-crate `UNMAKER_COUNTER`
                // exported by `#[plugin_aggregates]`. This avoids file I/O and
                // allows the host test to read the counter via a library symbol.
                // Note: `#[plugin_aggregates]` must be applied in the crate so the
                // `UNMAKER_COUNTER` symbol exists; otherwise this will fail to
                // compile for that crate.
                crate::UNMAKER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                if !vtable_ptr.is_null() {
                    ((*vtable_ptr).drop)((*vtable_ptr).user_data);
                    let _ = Box::from_raw(vtable_ptr);
                }
            }
        }

        // Submit this register function into the crate-local inventory type
        // named `__RegistrationFactory_<Trait>` as an erased extern "C" fn pointer.
        // The crate should apply `#[plugin_aggregates(Trait)]` once to declare
        // the `__RegistrationFactory_<Trait>` type; here we assume that type
        // exists and simply submit the function pointer.
        inventory::submit! {
            plugin_interface::RegistrationFactory {
                maker: #register_ident as extern "C" fn() -> *const std::ffi::c_void,
                unmaker: #unregister_ident as extern "C" fn(*const std::ffi::c_void),
                trait_name: #trait_name_lit.as_ptr() as *const std::os::raw::c_char,
            }
        }

        // Note: aggregated register_all/unregister_all helpers are generated by the
        // `#[plugin_aggregates(TraitName)]` attribute and are not emitted here to
        // avoid duplicate symbol definitions when a crate contains multiple
        // implementations. Each impl contributes a local inventory entry via the
        // `#local_factory_ident` type above.
    };

    TokenStream::from(expanded)
}

/// Emit aggregated register_all/unregister_all helpers for a trait by iterating
/// the crate-local inventory entries produced by `#[plugin_impl]` expansions.
#[proc_macro_attribute]
pub fn plugin_aggregates(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Expect the attribute to be the trait identifier, e.g. #[plugin_aggregates(Greeter)]
    let trait_path: syn::Path = parse_macro_input!(attr as syn::Path);
    let trait_ident = trait_path
        .segments
        .last()
        .expect("expected trait identifier")
        .ident
        .to_string();

    // literal for trait name, used in generated code comparisons
    let trait_name_lit = proc_macro2::Literal::string(&trait_ident);
    let register_all_symbol = format!("plugin_register_all_{}_v1", trait_ident);
    let register_all_ident = Ident::new(&register_all_symbol, proc_macro2::Span::call_site());
    let unregister_all_symbol = format!("plugin_unregister_all_{}_v1", trait_ident);
    let unregister_all_ident = Ident::new(&unregister_all_symbol, proc_macro2::Span::call_site());

    // Create a versioned getter symbol for the unmaker counter, e.g.
    // `plugin_unmaker_counter_Greeter_v1` so hosts can call a stable, typed API.
    let getter_symbol = format!("plugin_unmaker_counter_{}_v1", trait_ident);
    let getter_ident = Ident::new(&getter_symbol, proc_macro2::Span::call_site());

    // We iterate over plugin_interface::RegistrationFactory and filter by trait_name.

    let input_item: syn::Item = syn::parse(item).expect("failed to parse input item");
    let expanded = quote! {
    #input_item

    // Crate-level private counter that unmakers will increment. We emit a
    // `no_mangle` extern "C" getter that returns the counter value as `u64`.
    // We use `AtomicU64` for a fixed-width, cross-platform integer size.
    static UNMAKER_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    #[no_mangle]
    pub extern "C" fn #getter_ident() -> u64 {
        // Return the current counter value (atomic load as u64).
        UNMAKER_COUNTER.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[no_mangle]
    pub extern "C" fn #register_all_ident() -> *const plugin_interface::RegistrationArray {
            unsafe {
                let mut regs: Vec<*const std::ffi::c_void> = Vec::new();
                let mut factories: Vec<*const plugin_interface::RegistrationFactory> = Vec::new();
                for factory in inventory::iter::<plugin_interface::RegistrationFactory> {
                    // Filter by the trait name
                    let tn = std::ffi::CStr::from_ptr(factory.trait_name);
                    if let Ok(s) = tn.to_str() {
                        if s == #trait_name_lit {
                            let r = (factory.maker)();
                            if !r.is_null() {
                                regs.push(r as *const std::ffi::c_void);
                                factories.push(factory as *const plugin_interface::RegistrationFactory);
                            }
                        }
                    }
                }

                if regs.is_empty() {
                    return std::ptr::null();
                }

                let count = regs.len();
                let regs_box = regs.into_boxed_slice();
                let regs_ptr = Box::into_raw(regs_box) as *const *const std::ffi::c_void;

                let factories_box = factories.into_boxed_slice();
                let factories_ptr = Box::into_raw(factories_box) as *const *const plugin_interface::RegistrationFactory;

                let arr = Box::new(plugin_interface::RegistrationArray { count, registrations: regs_ptr, factories: factories_ptr });
                Box::into_raw(arr)
            }
        }

        #[no_mangle]
        pub extern "C" fn #unregister_all_ident(arr_ptr: *const plugin_interface::RegistrationArray) {
            if arr_ptr.is_null() { return; }
            unsafe {
                let arr_box: Box<plugin_interface::RegistrationArray> = Box::from_raw(arr_ptr as *mut _);
                let regs_ptr = arr_box.registrations as *mut *const std::ffi::c_void;
                let count = arr_box.count as usize;
                if !regs_ptr.is_null() && count > 0 {
                    let slice = std::slice::from_raw_parts_mut(regs_ptr, count);
                    let boxed_slice: Box<[*const std::ffi::c_void]> = Box::from_raw(slice as *mut [_]);

                    // For each registration pointer we need to call the corresponding
                    // unmaker function. We find unmakers by iterating the collected
                    // RegistrationFactory entries and matching the trait_name; for each
                    // factory we call its unmaker on the registrations it contributed.
                    // This relies on plugin authors arranging that their maker returns
                    // registrations that their unmaker understands.
                    let mut idx = 0usize;
                    for &r in boxed_slice.iter() {
                        if r.is_null() { idx += 1; continue; }

                        // Find the next factory that matches this trait and call its unmaker.
                        // In most cases there will be a one-to-one ordering between factories
                        // and registrations as produced by register_all; we conservatively
                        // scan factories and call unmaker for each registration matching the trait.
                        for factory in inventory::iter::<plugin_interface::RegistrationFactory> {
                            let tn = std::ffi::CStr::from_ptr(factory.trait_name);
                            if let Ok(s) = tn.to_str() {
                                if s == #trait_name_lit {
                                    (factory.unmaker)(r);
                                    break;
                                }
                            }
                        }

                        idx += 1;
                    }

                    // boxed_slice was allocated by register_all; drop it now to avoid leak.
                    // The individual registrations are freed by the unmaker calls above.
                    drop(boxed_slice);
                }
            }
        }
    };

    TokenStream::from(expanded)
}
