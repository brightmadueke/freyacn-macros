use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Ident,
    *,
};

/// This macro comes in the form of:
/// #[component]
/// fn CustomComponent(prop1: String, prop2: i32) {
///
/// }
/// it gives the
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let ItemFn {
        mut attrs,
        vis,
        sig,
        block,
    } = input;

    let fn_name = to_pascal_case(&sig.ident.to_string());
    let fn_visibility = vis.clone();
    let fn_block = block.clone();

    // collect (name, type) for each typed param
    let fn_params: Vec<(Ident, Type)> = sig.inputs.iter().filter_map(|arg| {
        if let FnArg::Typed(pt) = arg {
            if let Pat::Ident(pi) = &*pt.pat {
                return Some((pi.ident.clone(), (*pt.ty).clone()));
            } else {
                panic!("Parameters must be simple `name: Type` bindings")
            }
        }
        None
    }).collect();

    // Format function name to give the struct name. the structname is always in
    // PascalCase and has 'Component' at the end of its name
    let struct_name = format!("{}Component", fn_name.to_string());

    // Emit one field per parameter, preserving type and name
    let struct_fields = fn_params.iter().map(|(name, ty)| {
        quote! {pub #name: #ty,}
    });

    // Constructor that takes the same params
    let constructor_params = fn_params.iter().map(|(name, ty)| {
        quote! {#name: #ty,}
    });
    let constructor_inits = fn_params.iter().map(|(name, _)| {
        quote! {#name}
    });

    // names of the parameters, in declaration order
    let param_names: Vec<Ident> = fn_params.iter().map(|(name, _)| name.clone()).collect();

    quote! {
        #[derive(Debug, PartialEq, Clone)]
        struct #struct_name {
            elements: Vec<Element>,
            #(#struct_fields),*

           /*
            fn new(#(#constructor_params),*) -> Self {
                Self {#(#constructor_inits,)*}
            }
            */
        }

        impl ::freya::elements::extensions::ChildrenExt for #struct_name {
            fn get_children(&mut self) -> &mut Vec<Element> {
                &mut self.elements
            }
        }

        impl ::freya::elements::extensions::KeyExt for #struct_name {
            fn write_key(&mut self) -> &mut DiffKey {
                &mut self.key
            }
        }

        #fn_visibility fn #fn_name(#sig.inputs) -> ::freya::prelude::IntoElement {
            let component_struct = #struct_name::new { #(#param_names),* }
        }
    }.into()
}

/// convert `snake_case` to `PascalCase`
/// Handles leading underscores and digits gracefully
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|s| !s.is_empty())
        .map(|seg| {
            let mut chars = seg.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                }
            }
        })
        .collect()
}