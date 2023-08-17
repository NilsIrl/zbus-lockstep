//! # zbus-lockstep-macros
//!
//! This crate provides the `validate` macro that builds on `zbus-lockstep`.
#![doc(html_root_url = "https://docs.rs/zbus-lockstep-macros/0.1.0")]

type Result<T> = std::result::Result<T, syn::Error>;

use std::{collections::HashMap, path::PathBuf, str::FromStr};

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse::ParseStream, parse_macro_input, Ident, ItemStruct, LitStr, Token};

/// Validate a struct's type signature against signal body type.
///
/// Tries to get the signal body type from an XML file using the least amount
/// of information possible.
///
/// # Arguments
///
/// ## `xml`
///
/// Assumes XML file(s) in `xml/` or `XML/` of the crate root,
/// otherwise the path to the XML file must be provided.
///
/// Alternatively, you can provide the XML file as environment variable,
/// `ZBUS_LOCKSTEP_XML_PATH`, which will precede the path argument and the default path.
///
/// ## `interface`
///
/// If multiple interfaces contain a signal name that is contained in
/// the structs' identifier, the macro will fail and you can provide an
/// interface name to disambiguate.
///
/// ## `signal`
///
/// If a custom signal name is required, it may be provided `signal:`.
///
/// `#[validate(xml: <xml_path>, interface: <interface_name>, member: <member_name>)]`
///
/// # Examples
///
/// ```rust
/// 
/// use zbus_lockstep_macros::validate;
/// use zbus::zvariant::{OwnedObjectPath, Type};
///
/// #[validate(xml: "zbus-lockstep-macros/tests/xml")]
/// #[derive(Type)]
/// struct RemoveNodeHappening {
///    name: String,
///    path: OwnedObjectPath,
/// }
/// ```
#[proc_macro_attribute]
pub fn validate(args: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the macro arguments.
    let args = parse_macro_input!(args as ValidateArgs);

    // Parse the item struct.
    let item_struct = parse_macro_input!(input as ItemStruct);
    let item_name = item_struct.ident.to_string();

    let mut xml = args.xml;
    let interface = args.interface;
    let signal_arg = args.signal;

    resolve_xml_path(&mut xml);

    // If no path could be found, return a helpful error message.
    if xml.is_none() {
        let mut path = std::env::current_dir().unwrap();
        path.push("xml");
        let path = path.to_str().unwrap();

        let mut alt_path = std::env::current_dir().unwrap();
        alt_path.push("XML");
        let alt_path = alt_path.to_str().unwrap();

        let error_message = format!(
            "No XML file provided and no default XML file found in \"{}\" or \"{}\"",
            path, alt_path
        );

        return syn::Error::new(proc_macro2::Span::call_site(), error_message)
            .to_compile_error()
            .into();
    }

    // Safe to unwrap because we checked that it is not `None`.
    let xml = xml.unwrap();

    // Store each file's XML as a string in a with the XML's file path as key.
    let mut xml_files: HashMap<PathBuf, String> = HashMap::new();
    let read_dir = std::fs::read_dir(&xml);

    // If the path does not exist, the process lacks permissions to read the path,
    // or the path is not a directory, return an error.
    if let Err(e) = read_dir {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("Failed to read XML directory: {}", e),
        )
        .to_compile_error()
        .into();
    }

    // Iterate over the directory and store each XML file as a string.
    for file in read_dir.unwrap() {
        let file = file.expect("Failed to read XML file");

        if file.path().extension().expect("File has no extension.") == "xml" {
            let xml =
                std::fs::read_to_string(file.path()).expect("Unable to read XML file to string");
            xml_files.insert(file.path().clone(), xml);
        }
    }

    // These are later needed to call `get_signal_body_type`.
    let mut signal_name = None;
    let mut interface_name = interface;
    let mut xml_path = None;

    // Iterate over `xml_files` and find the signal that is contained in the struct's name.
    // Or if `signal_arg` is provided, use that.
    for (path_key, xml_string) in xml_files {
        let node = zbus::xml::Node::from_str(&xml_string).expect("Failed to parse XML file");

        let interfaces = node.interfaces();
        for interface in interfaces {
            let signals = interface.signals();
            for signal in signals {
                let xml_signal_name = signal.name();

                if signal_arg.is_some() && signal_name == signal_arg {
                    // If in an earlier iteration we already found a signal with the same name,
                    // error.
                    if interface_name.is_some() {
                        return syn::Error::new(
                            proc_macro2::Span::call_site(),
                            "Multiple interfaces with the same signal name. Please disambiguate.",
                        )
                        .to_compile_error()
                        .into();
                    }
                    interface_name = Some(interface.name().to_string());
                    signal_name = Some(xml_signal_name.to_string());
                    xml_path = Some(path_key.clone());
                }

                if item_name.contains(xml_signal_name) {
                    // If in an earlier iteration we already found a signal with the same name,
                    // error.
                    if interface_name.is_some() {
                        return syn::Error::new(
                            proc_macro2::Span::call_site(),
                            "Multiple interfaces with the same signal name. Please disambiguate.",
                        )
                        .to_compile_error()
                        .into();
                    }
                    interface_name = Some(interface.name().to_string());
                    signal_name = Some(xml_signal_name.to_string());
                    xml_path = Some(path_key.clone());
                }
            }
        }
    }

    // Lets be nice and provide a helpful error message.
    if interface_name.is_none() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "No interface with signal name '{}' found.",
                signal_arg.unwrap_or_else(|| item_name.clone())
            ),
        )
        .to_compile_error()
        .into();
    }

    if signal_name.is_none() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "No signal with name '{}' found.",
                signal_arg.unwrap_or_else(|| item_name.clone())
            ),
        )
        .to_compile_error()
        .into();
    }

    if xml_path.is_none() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "No XML file with signal name '{}' found.",
                signal_arg.unwrap_or_else(|| item_name.clone())
            ),
        )
        .to_compile_error()
        .into();
    }

    // Safe to unwrap because we checked that they are not `None`.
    let interface_name = interface_name.unwrap();
    let signal_name = signal_name.unwrap();
    let xml_path = xml_path.unwrap();
    let xml_path = xml_path.to_str().unwrap();

    // Create a block to return the item struct with a uniquely named validation test.
    let test_name = format!("test_{}_type_signature", item_name);
    let test_name = Ident::new(&test_name, proc_macro2::Span::call_site());

    let item_struct_name = item_struct.ident.clone();
    let item_struct_name = Ident::new(
        &item_struct_name.to_string(),
        proc_macro2::Span::call_site(),
    );

    let item_plus_validation_test = quote! {
        #item_struct

        #[test]
        fn #test_name() {
            use zbus::zvariant;
            use zbus::zvariant::Type;
            use zbus_lockstep::signatures_are_eq;
            use std::io::Cursor;

            let item_signature_from_xml = zbus_lockstep::get_signal_body_type(
                Cursor::new(#xml_path.as_bytes()),
                #interface_name,
                #signal_name,
                None
            ).expect("Failed to get signal body type from XML file");

            let item_signature_from_struct = <#item_struct_name as zvariant::Type>::signature();

            zbus_lockstep::assert_eq_signatures!(&item_signature_from_xml, &item_signature_from_struct);
        }
    };

    item_plus_validation_test.into()
}

struct ValidateArgs {
    // Optional path to XML file
    xml: Option<PathBuf>,

    // Optional interface name
    interface: Option<String>,

    // Optional signal name
    signal: Option<String>,
}

impl syn::parse::Parse for ValidateArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut xml = None;
        let mut interface = None;
        let mut signal = None;

        while !input.is_empty() {
            let ident = input.parse::<Ident>()?;
            match ident.to_string().as_str() {
                "xml" => {
                    input.parse::<Token![:]>()?;
                    let lit = input.parse::<LitStr>()?;
                    xml = Some(PathBuf::from(lit.value()));
                }
                "interface" => {
                    input.parse::<Token![:]>()?;
                    let lit = input.parse::<LitStr>()?;
                    interface = Some(lit.value());
                }
                "signal" => {
                    input.parse::<Token![:]>()?;
                    let lit = input.parse::<LitStr>()?;
                    signal = Some(lit.value());
                }
                _ => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("Unexpected argument: {}", ident),
                    ))
                }
            }
        }

        Ok(ValidateArgs {
            xml,
            interface,
            signal,
        })
    }
}

// TODO: Hardcoded paths may not be ideal.
fn resolve_xml_path(xml: &mut Option<PathBuf>) {
    // Try to find the XML file in the default locations.
    if xml.is_none() {
        let mut path = std::env::current_dir().unwrap();

        path.push("xml");
        if path.exists() {
            *xml = Some(path.clone());
        }

        path.pop();
        path.push("XML");
        if path.exists() {
            *xml = Some(path);
        }
    }

    // If the XML file is provided as environment variable.
    // This will override the default, so the env variable better be valid.
    if let Ok(env_path_xml) = std::env::var("ZBUS_LOCKSTEP_XML_PATH") {
        *xml = Some(PathBuf::from(env_path_xml));
    }
}
