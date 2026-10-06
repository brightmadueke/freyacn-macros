//! Procedural macros for the `freyacn` crate.
//!
//! # `#[component]`
//!
//! Turns a function into a component struct. Props come from two disjoint
//! sources, and the source determines the kind:
//!
//! | Declaration | Kind | Field | Body sees |
//! |---|---|---|---|
//! | `fn Card(name: String)` (parameter) | required | `Option<String>` | `&String` (panics if unset) |
//! | `#[component(name: String)]` | optional | `Option<String>` | `&Option<String>` |
//! | `#[component(name: String = "ada")]` | defaulted | `String` | `&String` |
//!
//! The user **does not** write `Option<T>` for optional props — writing
//! `name: String` in the attribute is enough; the macro wraps it in `Option`
//! and initialises to `None`.
//!
//! # `#[struct_fields(...)]`
//!
//! Adds one or more internal storage fields to the generated struct. No
//! setters, no public API — the fields exist so the user can write their own
//! extension trait impls that store custom data on the component.
//!
//! ```ignore
//! #[component(name: String)]
//! #[struct_fields(
//!     tooltip: MyTooltip,           // Option<MyTooltip>, init None
//!     hover_count: usize = 0,       // usize, init 0
//! )]
//! fn Card(name: String) -> Element { … }
//! ```
//!
//! The fields are visible in the render body as locals:
//! `tooltip: &Option<MyTooltip>`, `hover_count: &usize`.
//!
//! # `#[extensions(...)]`
//!
//! Opts the struct into freyacn's built-in extension bundles:
//!
//! | Name | Trait | Field |
//! |---|---|---|
//! | `children` | `freyacn::ChildrenExt` | `children: Vec<Element>` |
//! | `key` | `freyacn::KeyExt` | `key: DiffKey` |
//! | `style` | `freyacn::StyleExt` | `style: Style` |
//! | `event_handlers` | `freyacn::EventHandlersExt` | `event_handlers: FxHashMap<EventName, EventHandlerType>` |
//!
//! Extension fields are appended after props and struct fields. **Any** prop
//! or struct field that shares a name with an extension field is a compile
//! error.
//!
//! # Attribute ordering
//!
//! `#[struct_fields(...)]` and `#[extensions(...)]` must appear **below**
//! `#[component(...)]`. If they're placed above, a stub attribute fires with a
//! friendly error explaining the correct order.
//!
//! # Forwarded attributes
//!
//! Every attribute on the function other than `#[doc]`, `#[extensions(...)]`,
//! and `#[struct_fields(...)]` is forwarded verbatim to the generated struct.
//! This includes `#[derive(...)]`, `#[cfg(...)]`, and custom attribute
//! macros. The `#[component]` macro does not bake any derives in.

mod component;
mod extension;
mod struct_fields;

use proc_macro::TokenStream;

use crate::component::parse_component;
use crate::extension::parse_extensions;

/// See the crate docs.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    match parse_component(attr.into(), item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// See the crate docs.
#[proc_macro_attribute]
pub fn extensions(attr: TokenStream, item: TokenStream) -> TokenStream {
    match parse_extensions(attr.into(), item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Stub for `#[struct_fields(...)]` when it's placed above `#[component]`.
///
/// When the ordering is correct, `#[component]` consumes the
/// `#[struct_fields(...)]` attribute before the compiler ever sees this
/// function, so this stub never fires.
#[proc_macro_attribute]
pub fn struct_fields(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    struct_fields::stub().into()
}