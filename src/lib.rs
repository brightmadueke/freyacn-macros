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
pub fn component(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    let ItemFn {
        attrs: _attrs,
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
    let _constructor_params = fn_params.iter().map(|(name, ty)| {
        quote! {#name: #ty,}
    });
    let _constructor_inits = fn_params.iter().map(|(name, _)| {
        quote! {#name}
    });

    // names of the parameters, in declaration order
    let param_names: Vec<Ident> = fn_params.iter().map(|(name, _)| name.clone()).collect();

    quote! {
        #[derive(Debug, PartialEq, Clone)]
        struct #struct_name {
            elements: Vec<Element>,
            key: ::freyacn::__private::DiffKey,
            corner_radius: f32,
            background: Option<::freyacn::__private::Color>,
            text_color: Option<::freyacn::__private::Color>,
            padding_override: Option<::freyacn::__private::Gaps>,
            margin_override: Option<::freyacn::__private::Gaps>,
            width_override: Option<::freyacn::__private::Size>,
            height_override: Option<::freyacn::__private::Size>,
            min_width_override: Option<::freyacn::__private::Size>,
            min_height_override: Option<::freyacn::__private::Size>,
            max_width_override: Option<::freyacn::__private::Size>,
            max_height_override: Option<::freyacn::__private::Size>,
            border_width: Option<f32>,
            border_color: Option<Color>,
            opacity: Option<f32>,
            shadow: Option<Shadow>,
            #(#struct_fields),*

           /*
            fn new(#(#constructor_params),*) -> Self {
                Self {#(#constructor_inits,)*}
            }
            */
        }

        impl ::freyacn::__private::ChildrenExt for #struct_name {
            fn get_children(&mut self) -> &mut Vec<::freyacn::__private::Element> {
                &mut self.elements
            }
        }

        impl ::freyacn::__private::KeyExt for #struct_name {
            fn write_key(&mut self) -> &mut ::freyacn::__private::DiffKey {
                &mut self.key
            }
        }

        impl ::freyacn::__private::BackgroundExt for #struct_name {
            fn background(mut self, color: ::freyacn::__private::Color) -> Self {
                self.background = Some(color);
                self
            }
        }

        impl ::freyacn::__private::ForegroundExt for #struct_name {
            fn color(mut self, color: ::freyacn::__private::Color) -> Self {
                self.text_color = Some(color);
                self
            }
        }

        impl ::freyacn::__private::SpacingExt for #struct_name {
            fn padding(mut self, gaps: impl Into<::freyacn::__private::Gaps>) -> Self {
                self.padding_override = Some(gaps.into());
                self
            }

            fn margin(mut self, gaps: impl Into<::freyacn::__private::Gaps>) -> Self {
                self.margin_override = Some(gaps.into());
                self
            }
        }

        impl ::freyacn::__private::SizingExt for #struct_name {
            fn width(mut self, size: impl Into<::freyacn::__private::Size>) -> Self {
                self.width_override = Some(size.into());
                self
            }

            fn height(mut self, size: impl Into<::freyacn::__private::Size>) -> Self {
                self.height_override = Some(size.into());
                self
            }

            fn min_width(mut self, size: impl Into<::freyacn::__private::Size>) -> Self {
                self.min_width_override = Some(size.into());
                self
            }

            fn min_height(mut self, size: impl Into<::freyacn::__private::Size>) -> Self {
                self.min_height_override = Some(size.into());
                self
            }

            fn max_width(mut self, size: impl Into<::freyacn::__private::Size>) -> Self {
                self.max_width_override = Some(size.into());
                self
            }

            fn max_height(mut self, size: impl Into<::freyacn::__private::Size>) -> Self {
                self.max_height_override = Some(size.into());
                self
            }
        }

        impl ::freyacn::__private::BorderExt for #struct_name {
            fn border_width(mut self, width: f32) -> Self {
                self.border_width = Some(width);
                self
            }

            fn border_color(mut self, color: ::freyacn::__private::Color) -> Self {
                self.border_color = Some(color);
                self
            }

            fn corner_radius(mut self, radius: impl Into<::freyacn::__private::CornerRadius>) -> Self {
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

        impl ::freyacn::__private::EffectsExt for #struct_name {
            fn opacity(mut self, opacity: f32) -> Self {
                self.opacity = Some(opacity);
                self
            }

            fn shadow(mut self, shadow: impl Into<Shadow>) -> Self {
                self.shadow = Some(shadow.into());
                self
            }
        }

        impl ::freyacn::__private::CornerRadiusExt for #struct_name {
            fn with_corner_radius(self, corner_radius: f32) -> Self {
                self.corner_radius(corner_radius)
            }
        }

        // ---- Color helpers ----
        fn color_with_alpha(color: ::freyacn::__private::Color, alpha: f32) -> ::freyacn::__private::Color {
            let r = color.r();
            let g = color.g();
            let b = color.b();
            let a = (alpha * 255.0) as u8;
            ::freyacn::__private::Color::from_argb(a, r, g, b)
        }

        fn blend_colors(base: ::freyacn::__private::Color, blend: ::freyacn::__private::Color, ratio: f32) -> ::freyacn::__private::Color {
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
            ::freyacn::__private::Color::from_argb((a * 255.0) as u8, r as u8, g as u8, b as u8)
        }

        impl ::freyacn::__private::Component for #struct_name {
            fn render(&self) -> impl::freyacn::__private::Intoelement {
                #fn_block
            }
        }


        #fn_visibility fn #fn_name(#sig.inputs) -> #struct_name {
            let component_struct = #struct_name {
                elements: Vec::new(),
                key: ::freyacn::__private::DiffKey::None,
                background: None,
                text_color: None,
                padding_override: None,
                margin_override: None,
                width_override: None,
                height_override: None,
                min_width_override: None,
                min_height_override: None,
                max_width_override: None,
                max_height_override: None,
                border_width: None,
                border_color: None,
                opacity: None,
                shadow: None,
                #(#param_names),*
            }
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

// todo write a god readme and lib.rs doc for the crate
// todo make every parameter f the function beome a method in the struct for builder patterns