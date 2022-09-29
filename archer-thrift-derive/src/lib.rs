//! Derive Thrift (de-)serialization logic from data structures.

#![deny(rust_2018_idioms, clippy::all, clippy::pedantic)]
#![warn(missing_docs, clippy::missing_docs_in_private_items)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::missing_panics_doc
)]

use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Data, DataEnum, DataStruct, DeriveInput, Field, Fields, GenericArgument,
    PathArguments, Type,
};

/// Derive the implementation of `ThriftDeserialize`.
#[proc_macro_derive(ThriftDeserialize)]
pub fn thrift_deserialize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = match input.data {
        Data::Struct(ref data) => derive_struct(&name, data),
        Data::Enum(ref data) => derive_enum(&name, data),
        Data::Union(_) => panic!("unions not supported"),
    };

    expanded.into()
}

/// Generate an implementation for enums.
fn derive_enum(name: &Ident, data: &DataEnum) -> TokenStream {
    let variants = data
        .variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            assert!(v.fields.is_empty(), "only simple enums supported");

            let i = i as i32;
            let name = &v.ident;

            quote! { #i => Self::#name }
        })
        .collect::<Vec<_>>();

    let error_message = format!("unknown {name} value `{{}}`");

    quote! {
        impl crate::ThriftDeserialize for #name {
            fn read(prot: &mut impl TInputProtocol) -> ::thrift::Result<Self> {
                Ok(match prot.read_i32()? {
                    #(#variants),*,
                    v => {
                        return Err(::thrift::Error::Protocol(::thrift::ProtocolError::new(
                            ::thrift::ProtocolErrorKind::InvalidData,
                            format!(#error_message, v),
                        )))
                    }
                })
            }
        }
    }
}

/// Generate an implementation for structs.
fn derive_struct(name: &Ident, data: &DataStruct) -> TokenStream {
    let fields = match data.fields {
        Fields::Named(ref fields) => fields
            .named
            .iter()
            .enumerate()
            .map(|(i, f)| FieldInfo::from_field(name, f, i))
            .collect(),
        Fields::Unnamed(_) => panic!("unnamed structs not supported"),
        Fields::Unit => Vec::new(),
    };

    let fields_map = fields
        .iter()
        .filter_map(|f| f.required.then_some(&f.lookup_name));
    let errors_map = fields.iter().filter_map(|f| {
        f.required.then(|| {
            let error_name = &f.error_name;
            let lookup_name = &f.lookup_name;
            quote! { crate::jaeger::verify_read(#error_name, #lookup_name) }
        })
    });

    let matches = fields.iter().map(FieldInfo::to_match);

    quote! {
        impl crate::ThriftDeserialize for #name {
            fn read(prot: &mut impl TInputProtocol) -> ::thrift::Result<Self> {
                prot.read_struct_begin()?;
                let mut value = Self::default();
                #(let mut #fields_map = false);*;

                loop {
                    let ident = prot.read_field_begin()?;
                    if ident.field_type == ::thrift::protocol::TType::Stop {
                        break;
                    }

                    match ::thrift::protocol::field_id(&ident)? {
                        #(#matches)*
                        _ => prot.skip(ident.field_type)?,
                    }

                    prot.read_field_end()?;
                }

                prot.read_struct_end()?;

                #(#errors_map?);*;

                Ok(value)
            }
        }
    }
}

/// Check whether the type is likely to be an [`Option`].
fn is_option(ty: &Type) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .first()
            .map_or(false, |seg| seg.ident == "Option"),
        _ => false,
    }
}

/// Extract the inner type from a genertic type with a single generic type argument
/// (like `Option<u32>`, or `Vec<String>`).
fn inner_type(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Path(path) => {
            if path.path.segments.len() != 1 {
                return None;
            }

            match path.path.segments.first()?.arguments {
                PathArguments::AngleBracketed(ref args) => {
                    if args.args.len() != 1 {
                        return None;
                    }

                    match args.args.first()? {
                        GenericArgument::Type(ty) => Some(ty),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Information about a single struct field, that can generate all the source code to parse and
/// assign it from the Thrift payload.
struct FieldInfo<'a> {
    /// Name of this struct field.
    name: &'a Ident,
    /// Name of the boolean variable, that is used to track whether a required field was present in
    /// the encoded data payload.
    lookup_name: Ident,
    /// String version in the form `{struct}.{field}`, that is used for error reporting, when a
    /// required field was missing in the payload.
    error_name: String,
    /// Field index in the Thrift data.
    index: i16,
    /// The field's known type, which can be turned into a parsing statement of the right type.
    ty: KnownType<'a>,
    /// Whether the field must be present in the data payload.
    required: bool,
}

impl<'a> FieldInfo<'a> {
    /// Create the field info from given basic information. All other information is derived from
    /// these input parameters.
    fn from_field(struct_name: &Ident, field: &'a Field, index: usize) -> Self {
        let name = field.ident.as_ref().expect("missing field ident");
        let required = !is_option(&field.ty);

        Self {
            name,
            lookup_name: format_ident!("read_{name}"),
            error_name: format!("{struct_name}.{name}"),
            index: index as i16 + 1,
            ty: KnownType::from_type(if required {
                &field.ty
            } else {
                inner_type(&field.ty).expect("failed getting Option inner type")
            }),
            required,
        }
    }

    /// Create a match statement for the parsing loop of the struct. Fields can occur in random
    /// order and this parses and assigns a value when it is discovered in the payload.
    fn to_match(&self) -> TokenStream {
        let Self {
            name,
            lookup_name,
            error_name: _,
            index,
            ty,
            required,
        } = self;
        let read_impl = ty.read_impl();

        if *required {
            quote! {
                #index => {
                    value.#name = #read_impl?;
                    #lookup_name = true;
                }
            }
        } else {
            quote! {
                #index => value.#name = Some(#read_impl?),
            }
        }
    }
}

/// One of the known and supported types. These are types, that can be translated to source code for
/// parsing from Thrift's raw payload into the Rust type.
#[derive(Clone, Copy)]
enum KnownType<'a> {
    /// [`String`] value.
    String,
    /// [`bool`] value.
    Bool,
    /// 64-bit [`f64`] value.
    F64,
    /// 32-bit signed [`i32`] value.
    I32,
    /// 64-bit signed [`i64`] value.
    I64,
    /// [`Vec`] of [`u8`].
    VecU8,
    /// [`Vec`] of some type `T`, that is expected to implement the required Thrift deserialization
    /// trait.
    VecT(&'a Ident),
    /// Some external type `T`, that is expected to implement the required Thrift deserialization
    /// trait. Same as with the vector variant, this is a best effort, and just expected to
    /// implement the requried trait. If it turns out to not implement the trait, it'll result in
    /// a compile error.
    External(&'a Ident),
}

impl<'a> KnownType<'a> {
    /// Try to parse the given type into one of the known types, panicking in case it's none of
    /// them.
    fn from_type(ty: &'a Type) -> Self {
        match ty {
            Type::Path(path) => {
                assert!(
                    path.path.segments.len() == 1,
                    "path must have exactly 1 segment"
                );

                let segment = path.path.segments.first().unwrap();
                let name = &segment.ident;

                match name {
                    _ if name == "String" => Self::String,
                    _ if name == "bool" => Self::Bool,
                    _ if name == "f64" => Self::F64,
                    _ if name == "i32" => Self::I32,
                    _ if name == "i64" => Self::I64,
                    _ if name == "Vec" => match segment.arguments {
                        PathArguments::None => panic!("invalid Vec, without generic args"),
                        PathArguments::AngleBracketed(ref args) => {
                            assert!(
                                args.args.len() == 1,
                                "Vec must have exactly one type argument"
                            );

                            match args.args.first().unwrap() {
                                GenericArgument::Type(ty) => match ty {
                                    Type::Path(path) => {
                                        match path.path.get_ident().expect("path is not an ident") {
                                            name if name == "u8" => Self::VecU8,
                                            name => Self::VecT(name),
                                        }
                                    }
                                    _ => panic!("invalid type"),
                                },
                                _ => panic!("invalid generic argument"),
                            }
                        }
                        PathArguments::Parenthesized(_) => panic!("invalid Vec, with parenthesis"),
                    },
                    _ => Self::External(name),
                }
            }
            _ => panic!("type is not a path"),
        }
    }

    /// Generate the read implementation, that pulls data from the input stream and turns it into
    /// the right type.
    fn read_impl(self) -> TokenStream {
        match self {
            Self::String => quote! { prot.read_string() },
            Self::Bool => quote! { prot.read_bool() },
            Self::F64 => quote! { prot.read_double() },
            Self::I32 => quote! { prot.read_i32() },
            Self::I64 => quote! { prot.read_i64() },
            Self::VecU8 => quote! { prot.read_bytes() },
            Self::VecT(ident) => quote! { read_list::<#ident>(prot) },
            Self::External(ident) => quote! {
               <#ident as crate::ThriftDeserialize>::read(prot)
            },
        }
    }
}
