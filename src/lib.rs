#![doc = include_str!("../README.md")]

mod component;

use crate::component::parse_component;
use proc_macro::TokenStream;


#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    match parse_component(attr.into(), item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into()
    }
}