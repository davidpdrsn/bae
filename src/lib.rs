//! Coming soon

#![doc(html_root_url = "https://docs.rs/bae/0.0.1")]
#![allow(clippy::let_and_return)]
#![deny(
    unused_variables,
    mutable_borrow_reservation_conflict,
    dead_code,
    unused_must_use,
    unused_imports
)]

extern crate proc_macro;

use heck::SnakeCase;
use proc_macro2::TokenStream;
use proc_macro_error::*;
use quote::*;
use syn::{spanned::Spanned, *};

#[proc_macro_derive(FromAttributes, attributes())]
#[proc_macro_error]
pub fn from_attributes(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = parse_macro_input!(input as ItemStruct);
    FromAttributes::new(item).expand().into()
}

#[derive(Debug)]
struct FromAttributes {
    item: ItemStruct,
    tokens: TokenStream,
}

impl FromAttributes {
    fn new(item: ItemStruct) -> Self {
        Self {
            item,
            tokens: TokenStream::new(),
        }
    }

    fn expand(mut self) -> TokenStream {
        self.expand_from_attributes_method();
        self.expand_parse_impl();

        if std::env::var("BAE_DEBUG").is_ok() {
            eprintln!("{}", self.tokens);
        }

        self.tokens
    }

    fn struct_name(&self) -> &Ident {
        &self.item.ident
    }

    fn attr_name(&self) -> LitStr {
        let struct_name = self.struct_name();
        let name = struct_name.to_string().to_snake_case();
        LitStr::new(&name, struct_name.span())
    }

    fn expand_from_attributes_method(&mut self) {
        let struct_name = self.struct_name();
        let attr_name = self.attr_name();

        let code = quote! {
            impl #struct_name {
                pub fn from_attributes(attrs: &[syn::Attribute]) -> syn::Result<Self> {
                    use syn::spanned::Spanned;

                    for attr in attrs {
                        match attr.path.get_ident() {
                            Some(ident) if ident == #attr_name => {
                                return syn::parse2::<Self>(attr.tokens.clone());
                            }
                            // Ignore other attributes
                            _ => {},
                        }
                    }

                    if attrs.is_empty() {
                        Err(syn::Error::new(
                            proc_macro2::Span::call_site(),
                            &format!("missing attribute `#[{}]`", #attr_name),
                        ))
                    } else {
                        let full_span = attrs
                            .iter()
                            .fold(attrs[0].span(), |acc, attr| acc.join(attr.span()).unwrap());
                        Err(syn::Error::new(full_span, &format!("missing attribute `#[{}]`", #attr_name)))
                    }
                }
            }
        };
        self.tokens.extend(code);
    }

    fn expand_parse_impl(&mut self) {
        let struct_name = self.struct_name();
        let attr_name = self.attr_name();

        let variable_declarations = self.item.fields.iter().map(|field| {
            let name = &field.ident;
            quote! { let mut #name = std::option::Option::None; }
        });

        let match_arms = self.item.fields.iter().map(|field| {
            let field_name = get_field_name(field);
            let pattern = LitStr::new(&field_name.to_string(), field.span());

            if field_is_switch(field) {
                quote! {
                    #pattern => {
                        #field_name = std::option::Option::Some(());
                    }
                }
            } else {
                quote! {
                    #pattern => {
                        content.parse::<syn::Token![=]>()?;
                        #field_name = std::option::Option::Some(content.parse()?);
                    }
                }
            }
        });

        let unwrap_mandatory_fields = self
            .item
            .fields
            .iter()
            .filter(|field| !field_is_optional(field))
            .map(|field| {
                let field_name = get_field_name(field);
                let arg_name = LitStr::new(&field_name.to_string(), field.span());

                quote! {
                    let #field_name = if let std::option::Option::Some(#field_name) = #field_name {
                        #field_name
                    } else {
                        return syn::Result::Err(
                            input.error(
                                &format!("`#[{}]` is missing `{}` argument", #attr_name, #arg_name),
                            )
                        );
                    };
                }
            });

        let set_fields = self.item.fields.iter().map(|field| {
            let field_name = get_field_name(field);
            quote! { #field_name, }
        });

        let mut supported_args = self
            .item
            .fields
            .iter()
            .map(|field| get_field_name(field))
            .map(|field_name| format!("`{}`", field_name))
            .collect::<Vec<_>>();
        supported_args.sort_unstable();
        let supported_args = supported_args.join(", ");

        let code = quote! {
            impl syn::parse::Parse for #struct_name {
                #[allow(unreachable_code, unused_imports, unused_variables)]
                fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
                    #(#variable_declarations)*

                    let content;
                    syn::parenthesized!(content in input);

                    while !content.is_empty() {
                        let bae_attr_ident = content.parse::<syn::Ident>()?;

                        match &*bae_attr_ident.to_string() {
                            #(#match_arms)*
                            other => {
                                return syn::Result::Err(
                                    syn::Error::new(
                                        bae_attr_ident.span(),
                                        &format!(
                                            "`#[{}]` got unknown `{}` argument. Supported arguments are {}",
                                            #attr_name,
                                            other,
                                            #supported_args,
                                        ),
                                    )
                                );
                            }
                        }

                        content.parse::<syn::Token![,]>().ok();
                    }

                    #(#unwrap_mandatory_fields)*

                    syn::Result::Ok(Self { #(#set_fields)* })
                }
            }
        };
        self.tokens.extend(code);
    }
}

fn get_field_name(field: &Field) -> &Ident {
    field
        .ident
        .as_ref()
        .unwrap_or_else(|| abort!(field.span(), "Field without a name"))
}

fn field_is_optional(field: &Field) -> bool {
    let type_path = if let Type::Path(type_path) = &field.ty {
        type_path
    } else {
        return false;
    };

    let ident = &type_path
        .path
        .segments
        .last()
        .unwrap_or_else(|| abort!(field.span(), "Empty type path"))
        .ident;

    ident == "Option"
}

fn field_is_switch(field: &Field) -> bool {
    let unit_type = syn::parse_str::<Type>("()").unwrap();
    inner_type(&field.ty) == Some(&unit_type)
}

fn inner_type(ty: &Type) -> Option<&Type> {
    let type_path = if let Type::Path(type_path) = ty {
        type_path
    } else {
        return None;
    };

    let ty_args = &type_path
        .path
        .segments
        .last()
        .unwrap_or_else(|| abort!(ty.span(), "Empty type path"))
        .arguments;

    let ty_args = if let PathArguments::AngleBracketed(ty_args) = ty_args {
        ty_args
    } else {
        return None;
    };

    let generic_arg = &ty_args
        .args
        .last()
        .unwrap_or_else(|| abort!(ty_args.span(), "Empty generic argument"));

    let ty = if let GenericArgument::Type(ty) = generic_arg {
        ty
    } else {
        return None;
    };

    Some(ty)
}

#[cfg(test)]
mod test {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_ui() {
        let t = trybuild::TestCases::new();
        t.pass("tests/compile_pass/*.rs");
        t.compile_fail("tests/compile_fail/*.rs");
    }
}
