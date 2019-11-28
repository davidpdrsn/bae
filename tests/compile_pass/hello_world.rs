use bae::FromAttributes;

#[derive(Debug, Eq, PartialEq, FromAttributes)]
pub struct MyAttr {
    mandatory: syn::Type,
    optional_missing: Option<syn::Type>,
    optional_given: Option<syn::Type>,
    switch: Option<()>,
    ident: syn::Ident,
}

fn main() {
    use quote::*;
    use syn::*;

    let code = quote! {
        #[other_random_attr]
        #[my_attr(
            switch,
            ident = foo,
            mandatory = SomeType,
            optional_given = OtherType,
        )]
        struct Foo;
    };

    let item_struct = syn::parse2::<ItemStruct>(code).unwrap();
    let attrs = &item_struct.attrs;
    let my_attr = MyAttr::from_attributes(&attrs).unwrap();

    assert_eq!(
        my_attr.mandatory,
        syn::parse_str::<Type>("SomeType").unwrap()
    );

    assert_eq!(my_attr.optional_missing, None,);

    assert_eq!(
        my_attr.optional_given,
        Some(syn::parse_str::<Type>("OtherType").unwrap())
    );

    assert_eq!(my_attr.ident, syn::parse_str::<Ident>("foo").unwrap());

    assert_eq!(my_attr.switch.is_some(), true);
}
