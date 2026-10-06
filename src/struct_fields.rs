//! Stub attribute that fires when `#[struct_fields(...)]` is misplaced.

use proc_macro2::TokenStream as TokenStream2;

/// Emit a friendly compile error telling the user to move
/// `#[struct_fields(...)]` below `#[component(...)]`.
pub fn stub() -> TokenStream2 {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "`#[struct_fields(...)]` must be placed **below** `#[component(...)]`:\n\
         \n\
         ```ignore\n\
         #[component(name: String)]\n\
         #[struct_fields(tooltip: MyTooltip)]\n\
         fn Card(name: String) -> Element { … }\n\
         ```\n\
         \n\
         Attributes are processed top-to-bottom; `#[component]` consumes\n\
         `#[struct_fields]` and turns it into one or more struct fields. If\n\
         you place `#[struct_fields]` first, the compiler reaches it before\n\
         `#[component]` has had a chance to run.",
    )
        .to_compile_error()
}