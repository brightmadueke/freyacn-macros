//! Bundles of code generated for each `#[extensions(...)]` name.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Error, Fields, Ident, ItemStruct, Token, punctuated::Punctuated};

// ---------------------------------------------------------------------------
// Attribute argument parsing
// ---------------------------------------------------------------------------

pub struct ExtensionNames {
    pub names: Vec<String>,
}

impl Parse for ExtensionNames {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let parsed = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;
        Ok(Self {
            names: parsed.into_iter().map(|i| i.to_string()).collect(),
        })
    }
}

pub fn parse_names(attr: TokenStream2) -> syn::Result<Vec<String>> {
    Ok(syn::parse2::<ExtensionNames>(attr)?.names)
}

// ---------------------------------------------------------------------------
// Field-name lookup (used for collision detection)
// ---------------------------------------------------------------------------

pub fn field_names_for(name: &str) -> &'static [&'static str] {
    match name {
        "children" => &["children"],
        "key" => &["key"],
        "style" => &["style"],
        "event_handlers" => &["event_handlers"],
        _ => &[],
    }
}

pub fn collect_field_names(names: &[String]) -> Vec<String> {
    names
        .iter()
        .flat_map(|n| field_names_for(n).iter().map(|s| s.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// Extension bundle
// ---------------------------------------------------------------------------

struct Extension {
    fields: Vec<TokenStream2>,
    inits: Vec<TokenStream2>,
    locals: Vec<TokenStream2>,
    impls: Vec<TokenStream2>,
}

fn extension_for(name: &str, struct_name: &Ident) -> syn::Result<Extension> {
    let ext = match name {
        "children" => Extension {
            fields: vec![quote! {
                /// Child elements attached to this component.
                ///
                /// Added by `#[extensions(children)]`. Manipulate via
                /// [`freyacn::ChildrenExt`] or write into this field directly.
                pub children: ::std::vec::Vec<::freyacn::Element>
            }],
            inits: vec![quote! {
                children: ::std::vec::Vec::new()
            }],
            locals: vec![quote! {
                let children: &::std::vec::Vec<::freyacn::Element> = &self.children;
            }],
            impls: vec![quote! {
                impl ::freyacn::ChildrenExt for #struct_name {
                    fn get_children(&mut self) -> &mut ::std::vec::Vec<::freyacn::Element> {
                        &mut self.children
                    }
                }
            }],
        },

        "key" => Extension {
            fields: vec![quote! {
                /// The element's diff key.
                ///
                /// Added by `#[extensions(key)]`. Set via [`freyacn::KeyExt`]
                /// to help the reconciler track this element across renders.
                pub key: ::freyacn::DiffKey
            }],
            inits: vec![quote! {
                key: ::freyacn::DiffKey::None
            }],
            locals: vec![quote! {
                let key: &::freyacn::DiffKey = &self.key;
            }],
            impls: vec![quote! {
                impl ::freyacn::KeyExt for #struct_name {
                    fn write_key(&mut self) -> &mut ::freyacn::DiffKey {
                        &mut self.key
                    }
                }
            }],
        },

        "style" => Extension {
            fields: vec![quote! {
                /// Accumulated styling state.
                ///
                /// Added by `#[extensions(style)]`. Every Tailwind-style
                /// helper on [`freyacn::StyleExt`] mutates this field.
                pub style: ::freyacn::Style
            }],
            inits: vec![quote! {
                style: <::freyacn::Style as ::core::default::Default>::default()
            }],
            locals: vec![quote! {
                let style: &::freyacn::Style = &self.style;
            }],
            impls: vec![quote! {
                impl ::freyacn::StyleExt for #struct_name {
                    fn get_style(&mut self) -> &mut ::freyacn::Style {
                        &mut self.style
                    }
                }
            }],
        },

        "event_handlers" => Extension {
            fields: vec![quote! {
                /// Event-handler registry.
                ///
                /// Added by `#[extensions(event_handlers)]`. Provides
                /// `on_press`, `on_secondary_press`, … via
                /// [`freyacn::EventHandlersExt`].
                pub event_handlers:
                    ::freyacn::FxHashMap<::freyacn::EventName, ::freyacn::EventHandlerType>
            }],
            inits: vec![quote! {
                event_handlers: <::freyacn::FxHashMap<_, _> as ::core::default::Default>::default()
            }],
            locals: vec![quote! {
                let event_handlers: &::freyacn::FxHashMap<
                    ::freyacn::EventName,
                    ::freyacn::EventHandlerType,
                > = &self.event_handlers;
            }],
            impls: vec![quote! {
                impl ::freyacn::EventHandlersExt for #struct_name {
                    fn get_event_handlers(
                        &mut self,
                    ) -> &mut ::freyacn::FxHashMap<
                        ::freyacn::EventName,
                        ::freyacn::EventHandlerType,
                    > {
                        &mut self.event_handlers
                    }
                }
            }],
        },

        other => {
            return Err(Error::new(
                Span::call_site(),
                format!(
                    "Unsupported extension `{other}`. \
                     Valid names: `children`, `key`, `style`, `event_handlers`."
                ),
            ))
        }
    };
    Ok(ext)
}

// ---------------------------------------------------------------------------
// Collector
// ---------------------------------------------------------------------------

pub struct ExtensionsCode {
    pub field_defs: Vec<TokenStream2>,
    pub field_inits: Vec<TokenStream2>,
    pub local_bindings: Vec<TokenStream2>,
    pub trait_impls: Vec<TokenStream2>,
}

pub fn collect(struct_name: &Ident, names: &[String]) -> syn::Result<ExtensionsCode> {
    let mut out = ExtensionsCode {
        field_defs: Vec::new(),
        field_inits: Vec::new(),
        local_bindings: Vec::new(),
        trait_impls: Vec::new(),
    };

    for name in names {
        let ext = extension_for(name, struct_name)?;
        out.field_defs.extend(ext.fields);
        out.field_inits.extend(ext.inits);
        out.local_bindings.extend(ext.locals);
        out.trait_impls.extend(ext.impls);
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Standalone `#[extensions(...)]`
// ---------------------------------------------------------------------------

pub fn parse_extensions(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let item_struct: ItemStruct = syn::parse2(item)?;
    let struct_name = item_struct.ident.clone();

    let names = parse_names(attr)?;
    let code = collect(&struct_name, &names)?;

    let Fields::Named(named) = &item_struct.fields else {
        return Err(Error::new_spanned(
            &struct_name,
            "`#[extensions]` can only be used on structs with named fields",
        ));
    };

    let existing_fields = named.named.iter();
    let extra_fields = &code.field_defs;
    let impls = &code.trait_impls;

    let attrs = &item_struct.attrs;
    let vis = &item_struct.vis;
    let generics = &item_struct.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let ext_list = names.iter().map(|n| format!("* `{n}`")).collect::<Vec<_>>().join("\n");
    let generated_note = format!(
        "## Extensions\n\nThis struct has been extended with:\n\n{ext_list}"
    );

    Ok(quote! {
        #(#attrs)*
        #[doc = ""]
        #[doc = #generated_note]
        #vis struct #struct_name #generics #where_clause {
            #(#existing_fields,)*
            #(#extra_fields,)*
        }

        #(#impls)*

        #[allow(unused)]
        const _: fn() = || {
            fn _assert_generics #impl_generics () #where_clause {}
            let _ = _assert_generics::<#ty_generics>;
        };
    })
}