//! # PartialStruct
//!
//! This crate allows the user to generate a structure containing the
//! same fields as the original struct but wrapped in Option<T>.
//!
//! Examples:
//!
//! ```rust
//! use partial_struct::PartialStruct;
//!
//! #[derive(PartialStruct)]
//! struct Foo {
//!   meow: u32,
//!   woof: String,
//! }
//! ```
//!
//! will generate:
//!
//! ```rust
//! struct PartialFoo {
//!   meow: Option<u32>,
//!   woof: Option<String>,
//! }
//! ```
//!
//! ## Usage
//!
//! You can use this to generate a configuration for you program more easily.
//! If you use [toml-rs](https://github.com/alexcrichton/toml-rs) to parse your config file (using serde),
//! you'll need to wrap your values in Option<T>, or you need them present in the config file.
//! With this crate, you can easily generate your whole Config struct with an Option<T> wrap for each field.
//! This means that if a config is missing in the file, you'll get a None.
//!
//! ## Features
//!
//! * `Option<T>` inside the original structs are NOT handled. The generated
//! struct will be wrapped in an extra `Option` like `Option<Option<T>>`
//! * You can add derives to the generated struct:
//! ```rust
//! use partial_struct::PartialStruct;
//!
//! #[derive(PartialStruct, Debug, Clone, PartialEq)]
//! #[partial_struct(derive(Debug, Clone, PartialEq))]
//! struct Config;
//! ```
//! * You can also nest your generated struct by mapping the original types to their new names:
//! ```rust
//! use partial_struct::PartialStruct;
//!
//! #[derive(PartialStruct)]
//! struct Config {
//!     timeout: u32,
//!     #[partial_struct(ty = "PartialLogConfig")]
//!     log_config: LogConfig,
//! }
//!
//! #[derive(PartialStruct)]
//! struct LogConfig {
//!     log_file: String,
//!     log_level: usize,
//! }
//! ```
//! * You can skip wrapping fields in an option:
//! ```rust
//! use partial_struct::PartialStruct;
//!
//! #[derive(PartialStruct)]
//! struct Config {
//!     timeout: u32,
//!     #[partial_struct(skip)]
//!     duration: Option<u32>,
//! }
//! ```
//! * You can combine several options for the fields:
//! ```rust
//! use partial_struct::PartialStruct;
//! use serde::Deserialize;
//!
//! #[derive(PartialStruct)]
//! #[partial_struct(derive(Deserialize))]
//! struct Config {
//!     timeout: u32,
//!     #[partial_struct(skip)]
//!     #[partial_struct(serde(default))]
//!     duration: Option<u32>,
//! }
//! ```
extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(PartialStruct, attributes(partial_struct))]
pub fn partial_struct(input: TokenStream) -> TokenStream {
    let ast = syn::parse_macro_input!(input as syn::DeriveInput);
    let output = get_partial(&ast);
    let result = quote!(#output);

    result.into()
}

/// Get the partial struct from the struct given as input.
fn get_partial(input: &syn::DeriveInput) -> syn::DeriveInput {
    let ident = get_ident(&input.ident);
    let (attrs, _) = get_attributes(&input.attrs);
    let data = get_data(&input.data);
    let vis = input.vis.clone();
    let generics = input.generics.clone();

    syn::DeriveInput {
        ident,
        attrs,
        vis,
        generics,
        data,
    }
}

/// Get the attributes that will be used in the partial struct or its
/// fields. An additional `Action` is returned to indicate exactly
/// what to do in the case of a field, see [`Action`](Action).
fn get_attributes(orig: &[syn::Attribute]) -> (Vec<syn::Attribute>, Action) {
    let mut attrs = Vec::new();
    let mut action = Action::WrapWithOption;

    for attr in orig {
        match attr.parse_meta() {
            Ok(syn::Meta::List(ref meta)) if &*meta.ident.to_string() == "partial_struct" => {
                for nested in &meta.nested {
                    match nested {
                        syn::NestedMeta::Meta(syn::Meta::Word(ident)) if &*ident == "skip" => {
                            action = Action::Skip;
                        }
                        syn::NestedMeta::Meta(syn::Meta::NameValue(params)) => {
                            match params.ident.to_string().as_ref() {
                                "ty" => match params.lit {
                                    syn::Lit::Str(ref lit) => {
                                        let s = lit.value();
                                        let v =
                                            syn::parse_str(&s).expect("ty literal failed to parse");
                                        action = Action::ChangeToType(v);
                                    }
                                    _ => panic!("ty literal is not a string"),
                                },
                                name => {
                                    panic!("unknown name-value option {}", name);
                                }
                            }
                        }
                        _ => attrs.push(syn::parse_quote!(#[#nested])),
                    }
                }
            }
            Ok(syn::Meta::List(ref meta)) if &*meta.ident.to_string() == "protobuf_convert" => {
                // Avoid error that appears when ProtoBuf and PartialStruct are used together
            }
            _ => attrs.push(attr.clone()),
        }
    }

    (attrs, action)
}

/// Get the body that will be used for the partial struct. It iterates
/// over its fields, and depending on each field attributes one of
/// these actions will happen:
///
/// * The field is left as is (skip)
/// * The field is wrapped in an `Option` (default behaviour)
/// * The field is set to an specified type
fn get_data(orig: &syn::Data) -> syn::Data {
    let mut data = orig.clone();

    if let syn::Data::Struct(data) = &mut data {
        for field in data.fields.iter_mut() {
            let (attrs, action) = get_attributes(&field.attrs);

            match action {
                Action::Skip => (),
                Action::WrapWithOption => {
                    let orig_ty = field.ty.clone();
                    field.ty = syn::parse_quote!(Option<#orig_ty>);
                }
                Action::ChangeToType(ty) => {
                    field.ty = syn::parse_quote!(#ty);
                }
            }
            field.attrs = attrs;
        }
    }

    data
}

/// Get the name that will be used for the partial struct. It just
/// prepends `Partial` to the given identifier `ident`.
fn get_ident(ident: &syn::Ident) -> syn::Ident {
    let mut name = "Partial".to_string();
    name.push_str(&ident.to_string());

    syn::Ident::new(&name, ident.span())
}

/// Possible actions to take as a result of processing a field
/// attributes. See [`get_attributes`](get_attributes).
enum Action {
    Skip,
    WrapWithOption,
    ChangeToType(syn::Type),
}
