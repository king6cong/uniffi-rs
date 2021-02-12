/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Enum definitions for a `ComponentInterface`.
//!
//! This module converts enum definition from UDL into structures that can be
//! added to a `ComponentInterface`. A declaration in the UDL like this:
//!
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {};
//! enum Example {
//!   "one",
//!   "two"
//! };
//! # "##)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! Will result in a [`Enum`] member being added to the resulting [`ComponentInterface`]:
//!
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {};
//! # enum Example {
//! #   "one",
//! #   "two"
//! # };
//! # "##)?;
//! let e = ci.get_enum_definition("Example").unwrap();
//! assert_eq!(e.name(), "Example");
//! assert_eq!(e.variants().len(), 2);
//! assert_eq!(e.variants()[0].name(), "one");
//! assert_eq!(e.variants()[1].name(), "two");
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! Like in Rust, UniFFI enums can contain associated data, but this needs to be
//! declared with a different syntax in order to work within the restrictions of
//! WebIDL. A declaration like this:
//!
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {};
//! [Enum]
//! interface Example {
//!   Zero();
//!   One(u32 first);
//!   Two(u32 first, string second);
//! };
//! # "##)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! Will result in an [`Enum`] member whose variants have associated fields:
//!
//! ```
//! # let ci = uniffi_bindgen::interface::ComponentInterface::from_webidl(r##"
//! # namespace example {};
//! # [Enum]
//! # interface ExampleWithData {
//! #   Zero();
//! #   One(u32 first);
//! #   Two(u32 first, string second);
//! # };
//! # "##)?;
//! let e = ci.get_enum_definition("ExampleWithData").unwrap();
//! assert_eq!(e.name(), "ExampleWithData");
//! assert_eq!(e.variants().len(), 3);
//! assert_eq!(e.variants()[0].name(), "Zero");
//! assert_eq!(e.variants()[0].fields().len(), 0);
//! assert_eq!(e.variants()[1].name(), "One");
//! assert_eq!(e.variants()[1].fields().len(), 1);
//! assert_eq!(e.variants()[1].fields()[0].name(), "first");
//! # Ok::<(), anyhow::Error>(())
//! ```
use std::convert::TryFrom;

use anyhow::{bail, Result};

use super::record::Field;
use super::types::Type;
use super::{APIConverter, ComponentInterface};

/// Represents an enum with named variants, each of which may have named
/// and typed fields.
///
/// Enums are passed across the FFI by serializing to a bytebuffer, with a
/// i32 indicating the variant followed by the serialization of each field.
#[derive(Debug, Clone, Hash, Default)]
pub struct Enum {
    pub(super) name: String,
    pub(super) variants: Vec<Variant>,
    pub(super) docs: Vec<String>,
}

impl Enum {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn variants(&self) -> Vec<&Variant> {
        self.variants.iter().collect()
    }

    pub fn docs(&self) -> Vec<&str> {
        self.docs.iter().map(String::as_str).collect()
    }

    pub fn has_associated_data(&self) -> bool {
        self.variants.iter().any(Variant::has_fields)
    }
}

// Note that we have two `APIConverter` impls here - one for the `enum` case
// and one for the `[Enum] interface` case.

impl APIConverter<Enum> for weedle::EnumDefinition<'_> {
    fn convert(&self, _ci: &mut ComponentInterface) -> Result<Enum> {
        Ok(Enum {
            name: self.identifier.0.to_string(),
            variants: self
                .values
                .body
                .list
                .iter()
                .map::<Result<_>, _>(|v| {
                    Ok(Variant {
                        name: v.0.to_string(),
                        ..Default::default()
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            ..Default::default()
        })
    }
}

impl APIConverter<Enum> for weedle::InterfaceDefinition<'_> {
    fn convert(&self, ci: &mut ComponentInterface) -> Result<Enum> {
        if self.inheritance.is_some() {
            bail!("interface inheritence is not supported for enum interfaces");
        }
        // We don't need to check `self.attributes` here; if calling code has dispatched
        // to this impl then we already know there was an `[Enum]` attribute.
        Ok(Enum {
            name: self.identifier.0.to_string(),
            variants: self
                .members
                .body
                .iter()
                .map::<Result<Variant>, _>(|member| match member {
                    weedle::interface::InterfaceMember::Operation(t) => Ok(t.convert(ci)?),
                    _ => bail!(
                        "interface member type {:?} not supported in enum interface",
                        member
                    ),
                })
                .collect::<Result<Vec<_>>>()?,
            ..Default::default()
        })
    }
}

impl APIConverter<Enum> for &syn::ItemEnum {
    fn convert(&self, ci: &mut ComponentInterface) -> Result<Enum> {
        let attrs = super::synner::Attributes::try_from(&self.attrs)?;
        Ok(Enum {
            name: self.ident.to_string(),
            variants: self
                .variants
                .iter()
                .map(|v| v.convert(ci))
                .collect::<Result<Vec<_>>>()?,
            docs: attrs.docs,
        })
    }
}

/// Represents an individual variant in an Enum.
///
/// Each variant has a name and zero or more fields.
#[derive(Debug, Clone, Default, Hash)]
pub struct Variant {
    pub(super) name: String,
    pub(super) fields: Vec<Field>,
}

impl Variant {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn fields(&self) -> Vec<&Field> {
        self.fields.iter().collect()
    }

    pub fn has_fields(&self) -> bool {
        self.fields.len() > 0
    }
}

impl APIConverter<Variant> for weedle::interface::OperationInterfaceMember<'_> {
    fn convert(&self, ci: &mut ComponentInterface) -> Result<Variant> {
        if self.special.is_some() {
            bail!("special operations not supported");
        }
        if let Some(weedle::interface::StringifierOrStatic::Stringifier(_)) = self.modifier {
            bail!("stringifiers are not supported");
        }
        // OK, so this is a little weird.
        // The syntax we use for enum interface members is `Name(type arg, ...);`, which parses
        // as an anonymous operation where `Name` is the return type. We re-interpret it to
        // use `Name` as the name of the variant.
        if self.identifier.is_some() {
            bail!("enum interface members must not have a method name");
        }
        let name: String = {
            use weedle::types::{
                NonAnyType::Identifier, ReturnType, SingleType::NonAny, Type::Single,
            };
            match &self.return_type {
                ReturnType::Type(Single(NonAny(Identifier(id)))) => id.type_.0.to_owned(),
                _ => bail!("enum interface members must have plain identifers as names"),
            }
        };
        Ok(Variant {
            name,
            fields: self
                .args
                .body
                .list
                .iter()
                .map(|arg| arg.convert(ci))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl APIConverter<Variant> for &syn::Variant {
    fn convert(&self, ci: &mut ComponentInterface) -> Result<Variant> {
        super::synner::Attributes::try_from(&self.attrs)?;
        if self.discriminant.is_some() {
            bail!("Explicit enum discriminants are not supported");
        }
        let fields = match &self.fields {
            syn::Fields::Unit => vec![],
            syn::Fields::Unnamed(_) => bail!("Enum variants can only have named fields"),
            syn::Fields::Named(f) => f
                .named
                .iter()
                .map(|f| f.convert(ci))
                .collect::<Result<Vec<_>>>()?,
        };
        Ok(Variant {
            name: self.ident.to_string(),
            fields,
            ..Default::default()
        })
    }
}

impl APIConverter<Field> for weedle::argument::Argument<'_> {
    fn convert(&self, ci: &mut ComponentInterface) -> Result<Field> {
        match self {
            weedle::argument::Argument::Single(t) => t.convert(ci),
            weedle::argument::Argument::Variadic(_) => bail!("variadic arguments not supported"),
        }
    }
}

impl APIConverter<Field> for weedle::argument::SingleArgument<'_> {
    fn convert(&self, ci: &mut ComponentInterface) -> Result<Field> {
        let type_ = ci.resolve_type_expression(&self.type_)?;
        if let Type::Object(_) = type_ {
            bail!("Objects cannot currently be used in enum variant data");
        }
        if self.default.is_some() {
            bail!("enum interface variant fields must not have default values");
        }
        if self.attributes.is_some() {
            bail!("enum interface variant fields must not have attributes");
        }
        // TODO: maybe we should use our own `Field` type here with just name and type,
        // rather than appropriating record::Field..?
        Ok(Field {
            name: self.identifier.0.to_string(),
            type_,
            required: false,
            default: None,
        })
    }
}

impl APIConverter<Field> for &syn::Field {
    fn convert(&self, ci: &mut ComponentInterface) -> Result<Field> {
        super::synner::Attributes::try_from(&self.attrs)?;
        if !matches!(
            self.vis,
            syn::Visibility::Public(_) | syn::Visibility::Inherited
        ) {
            bail!("Variant fields must be public");
        }
        let name = match &self.ident {
            None => bail!("Variant fields must be named"),
            Some(id) => id.to_string(),
        };
        let type_ = ci.resolve_type_expression(&self.ty)?;
        if let Type::Object(_) = type_ {
            bail!("Objects cannot currently be used in enum variant data");
        }
        Ok(Field {
            name,
            type_,
            required: false,
            default: None,
        })
    }
}

#[cfg(test)]
mod test {
    use super::super::ffi::FFIType;
    use super::*;

    #[test]
    fn test_duplicate_variants() -> Result<()> {
        const UDL: &str = r#"
            namespace test{};
            // Weird, but currently allowed!
            // We should probably disallow this...
            enum Testing { "one", "two", "one" };
        "#;
        let ci = ComponentInterface::from_webidl(UDL).unwrap();
        assert_eq!(ci.iter_enum_definitions().len(), 1);
        assert_eq!(
            ci.get_enum_definition("Testing").unwrap().variants().len(),
            3
        );
        Ok(())
    }

    #[test]
    fn test_associated_data() -> Result<()> {
        const UDL: &str = r#"
            namespace test {
                void takes_an_enum(TestEnum e);
                void takes_an_enum_with_data(TestEnumWithData ed);
                TestEnum returns_an_enum();
                TestEnumWithData returns_an_enum_with_data();
            };
            enum TestEnum { "one", "two" };
            [Enum]
            interface TestEnumWithData {
                Zero();
                One(u32 first);
                Two(u32 first, string second);
            };
        "#;
        let ci = ComponentInterface::from_webidl(UDL).unwrap();
        assert_eq!(ci.iter_enum_definitions().len(), 2);
        assert_eq!(ci.iter_function_definitions().len(), 4);

        // The "plain old enum" with no associated data.
        let e = ci.get_enum_definition("TestEnum").unwrap();
        assert!(!e.has_associated_data());
        assert_eq!(e.variants().len(), 2);
        assert_eq!(
            e.variants().iter().map(|v| v.name()).collect::<Vec<_>>(),
            vec!["one", "two"]
        );
        assert_eq!(e.variants()[0].fields().len(), 0);
        assert_eq!(e.variants()[1].fields().len(), 0);

        // The enum with associated data.
        let ed = ci.get_enum_definition("TestEnumWithData").unwrap();
        assert!(ed.has_associated_data());
        assert_eq!(ed.variants().len(), 3);
        assert_eq!(
            ed.variants().iter().map(|v| v.name()).collect::<Vec<_>>(),
            vec!["Zero", "One", "Two"]
        );
        assert_eq!(ed.variants()[0].fields().len(), 0);
        assert_eq!(
            ed.variants()[1]
                .fields()
                .iter()
                .map(|f| f.name())
                .collect::<Vec<_>>(),
            vec!["first"]
        );
        assert_eq!(
            ed.variants()[1]
                .fields()
                .iter()
                .map(|f| f.type_())
                .collect::<Vec<_>>(),
            vec![Type::UInt32]
        );
        assert_eq!(
            ed.variants()[2]
                .fields()
                .iter()
                .map(|f| f.name())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert_eq!(
            ed.variants()[2]
                .fields()
                .iter()
                .map(|f| f.type_())
                .collect::<Vec<_>>(),
            vec![Type::UInt32, Type::String]
        );

        // Enums with no associated data pass over the FFI as bytebuffers.
        // (It might be nice to optimize these to pass as plain integers, but that's
        // difficult atop the current factoring of `ComponentInterface` and friends).
        let farg = ci.get_function_definition("takes_an_enum").unwrap();
        assert_eq!(farg.arguments()[0].type_(), Type::Enum("TestEnum".into()));
        assert_eq!(farg.ffi_func().arguments()[0].type_(), FFIType::RustBuffer);
        let fret = ci.get_function_definition("returns_an_enum").unwrap();
        assert!(matches!(fret.return_type(), Some(Type::Enum(nm)) if nm == "TestEnum"));
        assert!(matches!(
            fret.ffi_func().return_type(),
            Some(FFIType::RustBuffer)
        ));

        // Enums with associated data pass over the FFI as bytebuffers.
        let farg = ci
            .get_function_definition("takes_an_enum_with_data")
            .unwrap();
        assert_eq!(
            farg.arguments()[0].type_(),
            Type::Enum("TestEnumWithData".into())
        );
        assert_eq!(farg.ffi_func().arguments()[0].type_(), FFIType::RustBuffer);
        let fret = ci
            .get_function_definition("returns_an_enum_with_data")
            .unwrap();
        assert!(matches!(fret.return_type(), Some(Type::Enum(nm)) if nm == "TestEnumWithData"));
        assert!(matches!(
            fret.ffi_func().return_type(),
            Some(FFIType::RustBuffer)
        ));

        Ok(())
    }
}
