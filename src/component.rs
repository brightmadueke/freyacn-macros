//! Procedural macros for the `freyacn` crate.
//!
//! The `#[component]` attribute turns a plain function into a component
//! struct with:
//!
//! - one field per prop,
//! - a `new()` constructor and one `impl Into<T>`-accepting setter per prop,
//! - a `Default` impl delegating to `new()`,
//! - a `Render` impl whose body sees ergonomic prop bindings,
//! - `From<Struct> for Element`,
//! - a companion `macro_rules!` named after the function.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Expr, FnArg, Ident, ItemFn, Pat, PatType, Token, Type,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

// Custom keyword so users can write `required name: Type`.
mod kw {
    syn::custom_keyword!(required);
}

// ---------------------------------------------------------------------------
// Attribute parsing
// ---------------------------------------------------------------------------

/// A single declaration inside `#[props(...)]`:
///
/// ```text
/// [required] name[: Type][ = default_expr]
/// ```
struct PropDecl {
    required: bool,
    name: Ident,
    ty: Option<Type>,
    default: Option<Expr>,
}

impl Parse for PropDecl {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let required = if input.peek(kw::required) {
            input.parse::<kw::required>()?;
            true
        } else {
            false
        };

        let name: Ident = input.parse()?;

        let ty = if input.peek(Token![:]) {
            input.parse::<Token![:]>()?;
            Some(input.parse()?)
        } else {
            None
        };

        let default = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Some(input.parse()?)
        } else {
            None
        };

        Ok(Self { required, name, ty, default })
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
// Resolved prop
// ---------------------------------------------------------------------------

enum PropKind {
    /// `name: Type` — field is `Option<Type>`.
    /// Body binding is `Prop<'_, Type>` (or `Option<&Type>` if `Type` itself
    /// is `Option<...>`). No `Default` bound unless `T` is used via deref.
    Optional,

    /// `required name: Type` — field is `Option<Type>`.
    /// Body binding is `&Type`; panics at render time if never set.
    Required,

    /// `name: Type = expr` — field is `Type`, initialised in `new()`.
    /// Body binding is `&Type`.
    WithDefault(Expr),
}

struct Prop {
    name: Ident,
    ty: Type,
    kind: PropKind,
}


// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------


pub fn parse_component(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let func: ItemFn = syn::parse2(item)?;

    let fn_ident = format_ident!("{}", to_pascal_case(&func.sig.ident.to_string()));
    let struct_ident = format_ident!("{}Component", fn_ident);
    let vis = func.vis.clone();

    // ---- collect fn parameters (for type inference / required props) ----
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

    // ---- parse `#[component(props...)]` if present ----
    let PropsList(decls) = syn::parse2::<PropsList>(attr)?;
    let decls: Vec<PropDecl> = decls;

    // ---- resolve each declared prop ----
    let mut props: Vec<Prop> = Vec::new();

    for decl in decls {
        let ty = match decl.ty {
            Some(t) => t,
            None => fn_params
                .iter()
                .find(|(n, _)| *n == decl.name)
                .map(|(_, t)| t.clone())
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        &decl.name,
                        format!(
                            "prop `{}` has no type and no matching function parameter",
                            decl.name
                        ),
                    )
                })?,
        };

        let kind = if let Some(expr) = decl.default {
            PropKind::WithDefault(expr)
        } else if decl.required {
            PropKind::Required
        } else {
            PropKind::Optional
        };

        props.push(Prop {
            name: decl.name,
            ty,
            kind,
        });
    }

    // ---- fn params not listed in `#[component(props...)]` become required props ----
    for (name, ty) in &fn_params {
        if !props.iter().any(|p| &p.name == name) {
            props.push(Prop {
                name: name.clone(),
                ty: ty.clone(),
                kind: PropKind::Required,
            });
        }
    }

    // ---- struct fields ----
    let struct_fields = props.iter().map(|p| {
        let name = &p.name;
        let ty = &p.ty;
        quote! { #vis #name: ::freyacn::Property<#ty> }
    });

    // ---- `new()` initialisers ----
    let new_inits = props.iter().map(|p| {
        let name = &p.name;
        match &p.kind {
            PropKind::Optional => quote! {
                #name: ::freyacn::Property::optional()
            },
            PropKind::Required => quote! {
                #name: ::freyacn::Property::required()
            },
            PropKind::WithDefault(expr) => quote! {
                #name: ::freyacn::Property::with_default(#expr)
            },
        }
    });

    // ---- uniform setters on `Self` ----
    //
    // Every prop has the same signature:  `fn name(self, impl Into<T>) -> Self`
    // `Property::set` uses interior mutability, so `self` does not need to be
    // `mut`
    let setters = props.iter().map(|p| {
        let name = &p.name;
        let ty = &p.ty;
        quote! {
            #[allow(dead_code)]
            #vis fn #name(
                self,
                value: impl ::core::convert::Into<#ty>,
            ) -> Self {
                ::freyacn::Property::set(
                    &self.#name,
                    ::core::convert::Into::into(value),
                );
                self
            }
        }
    });

    // ---- uniform render-time bindings ----
    //
    // Every prop is bound as `&Property<T>`. The body reads with
    // `.get()`, `.with(...)`, `.get_ref()`, and mutates with `.set(...)`
    let render_bindings = props.iter().map(|p| {
        let name = &p.name;
        quote! {
            let #name = &self.#name;
        }
    });

    let body = &func.block;

    // ---- companion `macro_rules!` ----
    //
    // `#[macro_export]` hoists the macro to the crate root. The body refers
    // to `#struct_ident` unqualified, so at the call site the struct must be
    // in scope. This is the standard trade-off for proc-macro-generated
    // macros
    let companion_macro = quote! {
        /// Sugar for chaining setter calls on the generated component struct.
        ///
        /// ```ignore
        /// Card!()
        /// Card!(title = "hi")
        /// Card!(title = "hi", name = "there")
        /// ```
        #[macro_export]
        macro_rules! #fn_ident {
            ($($key:ident = $val:expr),* $(,)?) => {
                #struct_ident::new()$(.$key($val))*
            };
        }
    };

    // ---- final expansion ----
    let expanded = quote! {
        #[derive(::core::clone::Clone, ::core::cmp::PartialEq)]
        #vis struct #struct_ident {
            elements: Vec<Element>,
            key: ::freyacn::DiffKey,
            corner_radius: f32,
            background: ::core::option::Option<::freyacn::Color>,
            text_color: ::core::option::Option<::freyacn::Color>,
            padding_override: ::core::option::Option<::freyacn::Gaps>,
            margin_override: ::core::option::Option<::freyacn::Gaps>,
            width_override: ::core::option::Option<::freyacn::Size>,
            height_override: ::core::option::Option<::freyacn::Size>,
            min_width_override: ::core::option::Option<::freyacn::Size>,
            min_height_override: ::core::option::Option<::freyacn::Size>,
            max_width_override: ::core::option::Option<::freyacn::Size>,
            max_height_override: ::core::option::Option<::freyacn::Size>,
            border_width: ::core::option::Option<f32>,
            border_color: ::core::option::Option<Color>,
            opacity: ::core::option::Option<f32>,
            shadow: ::core::option::Option<Shadow>,
            #(#struct_fields,)*
        }

        impl #struct_ident {
            #[allow(dead_code)]
            #vis fn new() -> Self {
                Self {
                    corner_radius: 8.0,
                    elements: ::alloc::vec::Vec::new(),
                    key: ::freyacn::DiffKey::None,
                    background: ::core::option::Option::None,
                    text_color: ::core::option::Option::None,
                    padding_override: ::core::option::Option::None,
                    margin_override: ::core::option::Option::None,
                    width_override: ::core::option::Option::None,
                    height_override: ::core::option::Option::None,
                    min_width_override: ::core::option::Option::None,
                    min_height_override: ::core::option::Option::None,
                    max_width_override: ::core::option::Option::None,
                    max_height_override: ::core::option::Option::None,
                    border_width: ::core::option::Option::None,
                    border_color: ::core::option::Option::None,
                    opacity: ::core::option::Option::None,
                    shadow: ::core::option::Option::None,
                    #(#new_inits,)*
                }
            }

            #(#setters)*
        }

        impl ::core::default::Default for #struct_ident {
            fn default() -> Self {
                Self::new()
            }
        }
        
        impl ::freyacn::ChildrenExt for #struct_ident {
            fn get_children(&mut self) -> &mut Vec<::freyacn::Element> {
                &mut self.elements
            }
        }

        impl ::freyacn::KeyExt for #struct_ident {
            fn write_key(&mut self) -> &mut ::freyacn::DiffKey {
                &mut self.key
            }
        }

        impl ::freyacn::BackgroundExt for #struct_ident {
            fn background(mut self, color: ::freyacn::Color) -> Self {
                self.background = Some(color);
                self
            }
        }

        impl ::freyacn::ForegroundExt for #struct_ident {
            fn color(mut self, color: ::freyacn::Color) -> Self {
                self.text_color = Some(color);
                self
            }
        }

        impl ::freyacn::SpacingExt for #struct_ident {
            fn padding(mut self, gaps: impl Into<::freyacn::Gaps>) -> Self {
                self.padding_override = Some(gaps.into());
                self
            }

            fn margin(mut self, gaps: impl Into<::freyacn::Gaps>) -> Self {
                self.margin_override = Some(gaps.into());
                self
            }
        }

        impl ::freyacn::SizingExt for #struct_ident {
            fn width(mut self, size: impl Into<::freyacn::Size>) -> Self {
                self.width_override = Some(size.into());
                self
            }

            fn height(mut self, size: impl Into<::freyacn::Size>) -> Self {
                self.height_override = Some(size.into());
                self
            }

            fn min_width(mut self, size: impl Into<::freyacn::Size>) -> Self {
                self.min_width_override = Some(size.into());
                self
            }

            fn min_height(mut self, size: impl Into<::freyacn::Size>) -> Self {
                self.min_height_override = Some(size.into());
                self
            }

            fn max_width(mut self, size: impl Into<::freyacn::Size>) -> Self {
                self.max_width_override = Some(size.into());
                self
            }

            fn max_height(mut self, size: impl Into<::freyacn::Size>) -> Self {
                self.max_height_override = Some(size.into());
                self
            }
        }

        impl ::freyacn::BorderExt for #struct_ident {
            fn border_width(mut self, width: f32) -> Self {
                self.border_width = Some(width);
                self
            }

            fn border_color(mut self, color: ::freyacn::Color) -> Self {
                self.border_color = Some(color);
                self
            }

            fn corner_radius(mut self, radius: impl Into<::freyacn::CornerRadius>) -> Self {
                let radius = radius.into();
                let uniform = radius
                    .top_left
                    .max(radius.top_right)
                    .max(radius.bottom_left)
                    .max(radius.bottom_right);
                self.corner_radius = uniform;
                self
            }
        }

        impl ::freyacn::EffectsExt for #struct_ident {
            fn opacity(mut self, opacity: f32) -> Self {
                self.opacity = Some(opacity);
                self
            }

            fn shadow(mut self, shadow: impl Into<Shadow>) -> Self {
                self.shadow = Some(shadow.into());
                self
            }
        }

        impl ::freyacn::CornerRadiusExt for #struct_ident {
            fn with_corner_radius(self, corner_radius: f32) -> Self {
                self.corner_radius(corner_radius)
            }
        }

        // ---- Color helpers ----
        fn color_with_alpha(color: ::freyacn::Color, alpha: f32) -> ::freyacn::Color {
            let r = color.r();
            let g = color.g();
            let b = color.b();
            let a = (alpha * 255.0) as u8;
            ::freyacn::Color::from_argb(a, r, g, b)
        }

        fn blend_colors(base: ::freyacn::Color, blend: ::freyacn::Color, ratio: f32) -> ::freyacn::Color {
            let r1 = base.r() as f32;
            let g1 = base.g() as f32;
            let b1 = base.b() as f32;
            let a1 = base.a() as f32 / 255.0;
            let r2 = blend.r() as f32;
            let g2 = blend.g() as f32;
            let b2 = blend.b() as f32;
            let a2 = blend.a() as f32 / 255.0;
            let r = r1 + (r2 - r1) * ratio;
            let g = g1 + (g2 - g1) * ratio;
            let b = b1 + (b2 - b1) * ratio;
            let a = a1 + (a2 - a1) * ratio;
            ::freyacn::Color::from_argb((a * 255.0) as u8, r as u8, g as u8, b as u8)
        }

        impl ::freyacn::Component for #struct_ident {
            fn render(&self) -> impl ::freyacn::IntoElement {
                #(#render_bindings)*
                #body
            }
        }

        #companion_macro
    };

    Ok(expanded)
}

/// convert `snake_case` to `PascalCase`
/// Handles leading underscores and digits gracefully
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