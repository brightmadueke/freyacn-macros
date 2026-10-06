//! The `#[component]` attribute macro.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Expr, FnArg, Ident, ItemFn, Pat, PatType, Token, Type,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

use crate::extension;

// ---------------------------------------------------------------------------
// Attribute prop parsing
// ---------------------------------------------------------------------------
//
// Grammar (shared by `#[component(...)]` and `#[struct_fields(...)]`):
//
//     prop := ident ":" type [ "=" expr ]
//     list := prop { "," prop } [ "," ]

struct PropDecl {
    name: Ident,
    ty: Type,
    default: Option<Expr>,
}

impl Parse for PropDecl {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;

        if name == "required" {
            return Err(syn::Error::new_spanned(
                &name,
                "`required` is not a keyword. Function parameters are the only \
                 way to declare required props: `fn Card(name: String)`. \
                 Attribute props are optional (or defaulted with `= expr`).",
            ));
        }

        if !input.peek(Token![:]) {
            return Err(syn::Error::new_spanned(
                &name,
                "missing type — every field must declare its type, e.g. \
                 `name: String`. (Required props are declared via the function \
                 signature instead.)",
            ));
        }
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;

        let default = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Some(input.parse()?)
        } else {
            None
        };

        Ok(Self { name, ty, default })
    }
}

struct PropsList(Vec<PropDecl>);

impl Parse for PropsList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let decls = Punctuated::<PropDecl, Token![,]>::parse_terminated(input)?;
        Ok(Self(decls.into_iter().collect()))
    }
}

// ---------------------------------------------------------------------------
// Resolved fields
// ---------------------------------------------------------------------------

enum PropKind {
    /// From a function parameter. Stored as `Option<T>`, initialised to
    /// `None`, body sees `&T` (panics if unset).
    Required,
    /// No default. Stored as `Option<T>`, initialised to `None`, body sees
    /// `&Option<T>`.
    Optional,
    /// Has `= expr`. Stored as `T`, initialised with `expr`, body sees `&T`.
    WithDefault(Expr),
}

/// Where a field came from. Used to decide whether to emit a setter, how to
/// document the field, and how to phrase collision errors.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PropSource {
    /// `fn Card(name: String)` — a required prop.
    FnParam,
    /// `#[component(name: String)]` — an optional or defaulted prop.
    AttrProp,
    /// `#[struct_fields(name: Type)]` — internal storage.
    StructField,
}

fn source_label(source: PropSource) -> &'static str {
    match source {
        PropSource::FnParam => "a function parameter",
        PropSource::AttrProp => "an attribute prop (`#[component(...)]`)",
        PropSource::StructField => "a struct field (`#[struct_fields(...)]`)",
    }
}

struct Prop {
    name: Ident,
    ty: Type,
    kind: PropKind,
    source: PropSource,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn parse_component(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let mut func: ItemFn = syn::parse2(item)?;

    // =====================================================================
    // 0. Extract attributes.
    // =====================================================================

    // ---- 0a. `#[struct_fields(...)]` — pulled out first. ----
    let mut struct_field_decls: Vec<PropDecl> = Vec::new();
    let mut kept_attrs: Vec<Attribute> = Vec::new();
    for a in std::mem::take(&mut func.attrs) {
        if a.path().is_ident("struct_fields") {
            let decls = a
                .parse_args_with(Punctuated::<PropDecl, Token![,]>::parse_terminated)?;
            struct_field_decls.extend(decls);
        } else {
            kept_attrs.push(a);
        }
    }
    func.attrs = kept_attrs;

    // ---- 0b. Preserve user doc comments. ----
    let user_docs: Vec<Attribute> = func
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .cloned()
        .collect();

    // ---- 0c. `#[extensions(...)]` — pulled out second. ----
    let mut extension_names: Vec<String> = Vec::new();
    if let Some(pos) = func
        .attrs
        .iter()
        .position(|a| a.path().is_ident("extensions"))
    {
        let a = func.attrs.remove(pos);
        let tokens = a
            .meta
            .require_list()
            .map_err(|_| {
                syn::Error::new_spanned(
                    &a,
                    "expected `#[extensions(name1, name2, …)]`",
                )
            })?
            .tokens
            .clone();
        extension_names = extension::parse_names(tokens)?;
    }

    // ---- 0d. Everything remaining is forwarded to the struct. ----
    let forwarded_attrs: Vec<Attribute> = func
        .attrs
        .iter()
        .filter(|a| !a.path().is_ident("doc"))
        .cloned()
        .collect();

    // =====================================================================
    // 1. Names.
    // =====================================================================

    let fn_name_str = func.sig.ident.to_string();
    let fn_ident = format_ident!("{}", to_pascal_case(&fn_name_str));
    let struct_ident = format_ident!("{}Component", fn_ident);
    let vis = func.vis.clone();

    // =====================================================================
    // 2. Extension bundle.
    // =====================================================================

    let extensions_code = extension::collect(&struct_ident, &extension_names)?;
    let ext_field_defs = &extensions_code.field_defs;
    let ext_field_inits = &extensions_code.field_inits;
    let ext_local_bindings = &extensions_code.local_bindings;
    let ext_trait_impls = &extensions_code.trait_impls;
    let ext_field_names = extension::collect_field_names(&extension_names);

    // =====================================================================
    // 3. Function parameters (all required).
    // =====================================================================

    let fn_params: Vec<(Ident, Type)> = func
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(PatType { pat, ty, .. }) => match &**pat {
                Pat::Ident(pi) => Some((pi.ident.clone(), (**ty).clone())),
                _ => None,
            },
            _ => None,
        })
        .collect();

    // =====================================================================
    // 4. `#[component(...)]` argument.
    // =====================================================================

    let PropsList(decls) = syn::parse2::<PropsList>(attr)?;

    // =====================================================================
    // 5. Build `props` in declaration order:
    //       attribute props → struct fields → fn params.
    // =====================================================================

    let mut props: Vec<Prop> = Vec::new();

    // Attribute props.
    for decl in decls {
        let kind = match decl.default {
            Some(expr) => PropKind::WithDefault(expr),
            None => PropKind::Optional,
        };
        props.push(Prop {
            name: decl.name,
            ty: decl.ty,
            kind,
            source: PropSource::AttrProp,
        });
    }

    // Struct fields.
    for decl in struct_field_decls {
        let kind = match decl.default {
            Some(expr) => PropKind::WithDefault(expr),
            None => PropKind::Optional,
        };
        props.push(Prop {
            name: decl.name,
            ty: decl.ty,
            kind,
            source: PropSource::StructField,
        });
    }

    // Function parameters.
    for (name, ty) in &fn_params {
        props.push(Prop {
            name: name.clone(),
            ty: ty.clone(),
            kind: PropKind::Required,
            source: PropSource::FnParam,
        });
    }

    // =====================================================================
    // 6. Collision checks.
    // =====================================================================

    // 6a. Duplicate names anywhere in `props`.
    let mut seen: std::collections::HashMap<String, PropSource> =
        std::collections::HashMap::new();
    for prop in &props {
        let key = prop.name.to_string();
        if let Some(previous) = seen.get(&key) {
            return Err(syn::Error::new_spanned(
                &prop.name,
                format!(
                    "`{key}` is declared more than once ({} and {}). \
                     Each field name may be used by exactly one declaration.",
                    source_label(*previous),
                    source_label(prop.source),
                ),
            ));
        }
        seen.insert(key, prop.source);
    }

    // 6b. Extension fields are reserved by freyacn.
    for prop in &props {
        if ext_field_names.iter().any(|n| n == &prop.name.to_string()) {
            return Err(syn::Error::new_spanned(
                &prop.name,
                format!(
                    "`{}` collides with a field contributed by \
                     `#[extensions(...)]`. Extension fields are reserved by \
                     freyacn and cannot be shadowed by {}.",
                    prop.name,
                    source_label(prop.source),
                ),
            ));
        }
    }

    // =====================================================================
    // 7. Struct fields  (props → struct fields → extension fields).
    // =====================================================================

    let prop_fields = props.iter().map(|p| {
        let name = &p.name;
        let ty = &p.ty;
        let ty_str = quote!(#ty).to_string().replace(' ', "");

        let (storage_ty, doc) = match (&p.kind, p.source) {
            (PropKind::Required, _) => (
                quote! { ::core::option::Option<#ty> },
                format!(
                    "The `{name}` prop (`{ty_str}`).\n\n\
                     **Required.** Declared as a function parameter; stored as \
                     `Option<{ty_str}>` and initialised to `None`. The render \
                     body unwraps it and panics if it was never set.\n\n\
                     Set via [`Self::{name}`]."
                ),
            ),
            (PropKind::Optional, PropSource::StructField) => (
                quote! { ::core::option::Option<#ty> },
                format!(
                    "Internal storage field `{name}` (`{ty_str}`).\n\n\
                     **Struct field.** Declared via `#[struct_fields(...)]`; \
                     initialised to `None`. No setter is generated — the user \
                     manages this field through their own trait impls.\n\n\
                     The render body sees `&Option<{ty_str}>`."
                ),
            ),
            (PropKind::Optional, _) => (
                quote! { ::core::option::Option<#ty> },
                format!(
                    "The `{name}` prop (`{ty_str}`).\n\n\
                     **Optional.** Declared in `#[component]` without a \
                     default; stored as `Option<{ty_str}>` and initialised to \
                     `None`. The render body sees `&Option<{ty_str}>`.\n\n\
                     Set via [`Self::{name}`]."
                ),
            ),
            (PropKind::WithDefault(_), PropSource::StructField) => (
                quote! { #ty },
                format!(
                    "Internal storage field `{name}` (`{ty_str}`).\n\n\
                     **Struct field.** Declared via `#[struct_fields(...)]` with \
                     a default expression; initialised with it in \
                     [`Self::new`]. No setter is generated.\n\n\
                     The render body sees `&{ty_str}`."
                ),
            ),
            (PropKind::WithDefault(_), _) => (
                quote! { #ty },
                format!(
                    "The `{name}` prop (`{ty_str}`).\n\n\
                     **Defaulted.** Declared in `#[component]` with a default \
                     expression; stored as `{ty_str}` and initialised with it \
                     in [`Self::new`]. The render body sees `&{ty_str}`.\n\n\
                     Set via [`Self::{name}`]."
                ),
            ),
        };

        quote! {
            #[doc = #doc]
            #vis #name: #storage_ty
        }
    });

    // =====================================================================
    // 8. `new()` initialisers.
    // =====================================================================

    let prop_inits = props.iter().map(|p| {
        let name = &p.name;
        match &p.kind {
            PropKind::Required | PropKind::Optional => quote! {
                #name: ::core::option::Option::None
            },
            PropKind::WithDefault(expr) => quote! {
                #name: #expr
            },
        }
    });

    // =====================================================================
    // 9. Setters — attribute props and fn params only.
    // =====================================================================

    let prop_setters = props
        .iter()
        .filter(|p| p.source != PropSource::StructField)
        .map(|p| {
            let name = &p.name;
            let ty = &p.ty;
            let ty_str = quote!(#ty).to_string().replace(' ', "");

            let (accepts, doc) = match &p.kind {
                PropKind::Required => (
                    quote! { impl ::core::convert::Into<#ty> },
                    format!(
                        "Set the [`{name}`](Self::{name}) prop.\n\n\
                         Required prop — accepts any type convertible into \
                         `{ty_str}`. Passing `None` or `Some(…)` is a type \
                         error, which is intentional."
                    ),
                ),
                PropKind::Optional => (
                    quote! { impl ::core::convert::Into<::core::option::Option<#ty>> },
                    format!(
                        "Set the [`{name}`](Self::{name}) prop.\n\n\
                         Optional prop. Accepts a bare `{ty_str}` (auto-wrapped \
                         into `Some` via std's `From<T> for Option<T>`) or an \
                         `Option<{ty_str}>` directly."
                    ),
                ),
                PropKind::WithDefault(_) => (
                    quote! { impl ::core::convert::Into<#ty> },
                    format!(
                        "Set the [`{name}`](Self::{name}) prop.\n\n\
                         Defaulted prop — accepts any type convertible into \
                         `{ty_str}`."
                    ),
                ),
            };

            quote! {
                #[doc = #doc]
                #[allow(dead_code)]
                #vis fn #name(
                    mut self,
                    value: #accepts,
                ) -> Self {
                    self.#name = ::core::convert::Into::into(value);
                    self
                }
            }
        });

    // =====================================================================
    // 10. Render-body bindings.
    // =====================================================================

    let prop_bindings = props.iter().map(|p| {
        let name = &p.name;
        let ty = &p.ty;
        match &p.kind {
            PropKind::Required => quote! {
                let #name: &#ty = self
                    .#name
                    .as_ref()
                    .expect(::core::concat!(
                        "required prop `",
                        ::core::stringify!(#name),
                        "` was not set before render"
                    ));
            },
            PropKind::Optional => quote! {
                let #name: &::core::option::Option<#ty> = &self.#name;
            },
            PropKind::WithDefault(_) => quote! {
                let #name: &#ty = &self.#name;
            },
        }
    });

    let body = &func.block;

    // =====================================================================
    // 11. Generated documentation.
    // =====================================================================

    // 11a. Public prop summary.
    let prop_summary: Vec<String> = props
        .iter()
        .filter(|p| p.source != PropSource::StructField)
        .map(|p| {
            let ty = &p.ty;
            let ty_str = quote!(#ty).to_string().replace(' ', "");
            let kind = match &p.kind {
                PropKind::Required => "required (fn param)",
                PropKind::Optional => "optional",
                PropKind::WithDefault(_) => "with default",
            };
            format!("* `{}`: `{}` — {}", p.name, ty_str, kind)
        })
        .collect();

    let props_section = if prop_summary.is_empty() {
        "*No props.*".to_string()
    } else {
        prop_summary.join("\n")
    };

    // 11b. Struct field summary.
    let struct_field_summary: Vec<String> = props
        .iter()
        .filter(|p| p.source == PropSource::StructField)
        .map(|p| {
            let ty = &p.ty;
            let ty_str = quote!(#ty).to_string().replace(' ', "");
            let kind = match &p.kind {
                PropKind::Optional => "`None`",
                PropKind::WithDefault(_) => "with default",
                PropKind::Required => unreachable!(),
            };
            format!("* `{}`: `{}` — {}", p.name, ty_str, kind)
        })
        .collect();

    let struct_fields_section = if struct_field_summary.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n## Struct fields\n\n\
             Internal storage. No setters are generated; manage these fields \
             via your own trait impls.\n\n{}",
            struct_field_summary.join("\n"),
        )
    };

    // 11c. Extensions summary.
    let ext_section = if extension_names.is_empty() {
        "*No extensions.*".to_string()
    } else {
        extension_names
            .iter()
            .map(|n| format!("* `{n}`"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let struct_doc = format!(
        "Component generated from the function `{fn_name}`.\n\n\
         ## Props\n\n{props_section}\
         {struct_fields_section}\n\n\
         ## Extensions\n\n{ext_section}",
        fn_name = fn_name_str,
    );

    let new_doc = format!(
        "Create a new [`{struct_ident}`] with every prop in its initial state.\n\n\
         Chain setters to configure props:\n\n\
         ```ignore\n\
         {struct_ident}::new().prop1(value1).prop2(value2)\n\
         ```"
    );

    let ctor_doc = format!(
        "Construct a new [`{struct_ident}`].\n\n\
         Equivalent to [`{struct_ident}::new`]. Chain setters to configure props:\n\n\
         ```ignore\n\
         {fn_ident}().prop1(value1).prop2(value2)\n\
         ```"
    );

    let render_doc = format!(
        "Renders this [`{struct_ident}`].\n\n\
         Props are bound by reference as locals: required and defaulted props \
         as `&T`, optional props and struct fields as `&Option<T>`. Extension \
         fields are bound the same way."
    );

    let macro_doc = format!(
        "Sugar macro for constructing [`{struct_ident}`].\n\n\
         ```ignore\n\
         {fn_ident}!(prop1 = value1, prop2 = value2)\n\
         ```"
    );

    // =====================================================================
    // 12. Component impl.
    // =====================================================================

    let component_trait_impl = quote! {
        impl ::freyacn::Component for #struct_ident {
            #[doc = #render_doc]
            fn render(&self) -> impl ::freyacn::IntoElement {
                #(#ext_local_bindings)*
                #(#prop_bindings)*
                #body
            }
        }
    };

    // =====================================================================
    // 13. Companion macro.
    // =====================================================================

    let companion_macro = quote! {
        #[doc = #macro_doc]
        #[doc(hidden)]
        #[macro_export]
        macro_rules! #fn_ident {
            ($($key:ident = $val:expr),* $(,)?) => {
                #struct_ident::new()$(.$key($val))*
            };
        }
    };

    // =====================================================================
    // 14. Final expansion.
    // =====================================================================

    let expanded = quote! {
        // 1. User doc comments on the function.
        #(#user_docs)*
        // 2. Generated summary.
        #[doc = ""]
        #[doc = #struct_doc]
        // 3. Every other attribute the user wrote on the function, verbatim.
        #(#forwarded_attrs)*
        #vis struct #struct_ident {
            #(#prop_fields,)*
            #(#ext_field_defs,)*
        }

        impl #struct_ident {
            #[doc = #new_doc]
            #[allow(dead_code)]
            #vis fn new() -> Self {
                Self {
                    #(#prop_inits,)*
                    #(#ext_field_inits,)*
                }
            }

            #(#prop_setters)*
        }

        #[doc = "Default constructor — delegates to [`Self::new`]."]
        impl ::core::default::Default for #struct_ident {
            fn default() -> Self {
                Self::new()
            }
        }

        #[doc = #ctor_doc]
        #[allow(non_snake_case)]
        #vis fn #fn_ident() -> #struct_ident {
            #struct_ident::new()
        }

        #(#ext_trait_impls)*

        #component_trait_impl

        #companion_macro
    };

    Ok(expanded)
}

/// Convert `snake_case` to `PascalCase`.
pub fn to_pascal_case(s: &str) -> String {
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