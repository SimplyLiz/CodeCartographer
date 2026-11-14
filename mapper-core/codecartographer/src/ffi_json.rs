//! camelCase JSON at the C-ABI boundary.
//!
//! The FFI contract (lib.rs doc comments, CKB's `internal/cartographer/types.go`)
//! is camelCase, but most result structs (`ProjectGraphResponse`, `GraphNode`,
//! `SimulatedChange`, `LayerViolation`, `ArchitectureEvolution`,
//! `CoChangeDispersion`, ...) derive `Serialize` without `rename_all`, because
//! the MCP server and the CLI print them snake_case and clients rely on that.
//!
//! [`Camel`] wraps a value and re-serializes it through a serializer adapter
//! that renames **struct field names** (the `&'static str` keys serde hands to
//! `serialize_field`) to camelCase. Map keys go through `serialize_key` and are
//! left alone, so data keys — file paths in `git_churn`, extensions in
//! `metadata.languages` — can never be rewritten, whatever they look like.
//! Enum variant names and string values are untouched too.
//!
//! A `serde_json::Value` serializes its objects as maps, so wrapping an
//! already-built `json!` value is a no-op: structs embedded in a `json!` must be
//! converted with [`camel`] at the embedding site, before `json!` turns them
//! into maps.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{Mutex, OnceLock};

use serde::ser::{
    Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
};

/// Serialize `value` with struct field names in camelCase.
pub struct Camel<'a, T: ?Sized>(pub &'a T);

impl<T: ?Sized + Serialize> Serialize for Camel<'_, T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(CamelSer(s))
    }
}

/// Convert `value` to a `serde_json::Value` with camelCase struct field names.
pub fn camel<T: ?Sized + Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(Camel(value)).unwrap_or(serde_json::Value::Null)
}

/// `snake_case` → `camelCase`. Leading underscores are kept; a name without
/// underscores (already camelCase, or a single word) is returned unchanged.
pub fn to_camel_case(name: &str) -> String {
    let trimmed = name.trim_start_matches('_');
    let lead = &name[..name.len() - trimmed.len()];
    let mut out = String::with_capacity(name.len());
    out.push_str(lead);
    let mut upper = false;
    for ch in trimmed.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Interned camelCase form of a struct field name. Serde requires
/// `&'static str` field names; the set of field names in the program is
/// finite, so leaking one string per distinct name is bounded.
fn camel_key(name: &'static str) -> &'static str {
    if !name.contains('_') {
        return name;
    }
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();
    let mut map = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.entry(name)
        .or_insert_with(|| Box::leak(to_camel_case(name).into_boxed_str()))
}

struct CamelSer<S>(S);

/// Compound-serializer adapter: wraps every element/value in [`Camel`].
struct Compound<C>(C);

impl<S: Serializer> Serializer for CamelSer<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Compound<S::SerializeSeq>;
    type SerializeTuple = Compound<S::SerializeTuple>;
    type SerializeTupleStruct = Compound<S::SerializeTupleStruct>;
    type SerializeTupleVariant = Compound<S::SerializeTupleVariant>;
    type SerializeMap = Compound<S::SerializeMap>;
    type SerializeStruct = Compound<S::SerializeStruct>;
    type SerializeStructVariant = Compound<S::SerializeStructVariant>;

    fn serialize_bool(self, v: bool) -> Result<S::Ok, S::Error> { self.0.serialize_bool(v) }
    fn serialize_i8(self, v: i8) -> Result<S::Ok, S::Error> { self.0.serialize_i8(v) }
    fn serialize_i16(self, v: i16) -> Result<S::Ok, S::Error> { self.0.serialize_i16(v) }
    fn serialize_i32(self, v: i32) -> Result<S::Ok, S::Error> { self.0.serialize_i32(v) }
    fn serialize_i64(self, v: i64) -> Result<S::Ok, S::Error> { self.0.serialize_i64(v) }
    fn serialize_i128(self, v: i128) -> Result<S::Ok, S::Error> { self.0.serialize_i128(v) }
    fn serialize_u8(self, v: u8) -> Result<S::Ok, S::Error> { self.0.serialize_u8(v) }
    fn serialize_u16(self, v: u16) -> Result<S::Ok, S::Error> { self.0.serialize_u16(v) }
    fn serialize_u32(self, v: u32) -> Result<S::Ok, S::Error> { self.0.serialize_u32(v) }
    fn serialize_u64(self, v: u64) -> Result<S::Ok, S::Error> { self.0.serialize_u64(v) }
    fn serialize_u128(self, v: u128) -> Result<S::Ok, S::Error> { self.0.serialize_u128(v) }
    fn serialize_f32(self, v: f32) -> Result<S::Ok, S::Error> { self.0.serialize_f32(v) }
    fn serialize_f64(self, v: f64) -> Result<S::Ok, S::Error> { self.0.serialize_f64(v) }
    fn serialize_char(self, v: char) -> Result<S::Ok, S::Error> { self.0.serialize_char(v) }
    fn serialize_str(self, v: &str) -> Result<S::Ok, S::Error> { self.0.serialize_str(v) }
    fn serialize_bytes(self, v: &[u8]) -> Result<S::Ok, S::Error> { self.0.serialize_bytes(v) }
    fn serialize_none(self) -> Result<S::Ok, S::Error> { self.0.serialize_none() }
    fn serialize_some<T: ?Sized + Serialize>(self, v: &T) -> Result<S::Ok, S::Error> {
        self.0.serialize_some(&Camel(v))
    }
    fn serialize_unit(self) -> Result<S::Ok, S::Error> { self.0.serialize_unit() }
    fn serialize_unit_struct(self, name: &'static str) -> Result<S::Ok, S::Error> {
        self.0.serialize_unit_struct(name)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        idx: u32,
        variant: &'static str,
    ) -> Result<S::Ok, S::Error> {
        self.0.serialize_unit_variant(name, idx, variant)
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        v: &T,
    ) -> Result<S::Ok, S::Error> {
        self.0.serialize_newtype_struct(name, &Camel(v))
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        idx: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<S::Ok, S::Error> {
        self.0.serialize_newtype_variant(name, idx, variant, &Camel(v))
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, S::Error> {
        self.0.serialize_seq(len).map(Compound)
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, S::Error> {
        self.0.serialize_tuple(len).map(Compound)
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, S::Error> {
        self.0.serialize_tuple_struct(name, len).map(Compound)
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        idx: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, S::Error> {
        self.0.serialize_tuple_variant(name, idx, variant, len).map(Compound)
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, S::Error> {
        self.0.serialize_map(len).map(Compound)
    }
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, S::Error> {
        self.0.serialize_struct(name, len).map(Compound)
    }
    fn serialize_struct_variant(
        self,
        name: &'static str,
        idx: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, S::Error> {
        self.0.serialize_struct_variant(name, idx, variant, len).map(Compound)
    }
    fn collect_str<T: ?Sized + Display>(self, v: &T) -> Result<S::Ok, S::Error> {
        self.0.collect_str(v)
    }
    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

impl<C: SerializeSeq> SerializeSeq for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), C::Error> {
        self.0.serialize_element(&Camel(v))
    }
    fn end(self) -> Result<C::Ok, C::Error> { self.0.end() }
}

impl<C: SerializeTuple> SerializeTuple for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), C::Error> {
        self.0.serialize_element(&Camel(v))
    }
    fn end(self) -> Result<C::Ok, C::Error> { self.0.end() }
}

impl<C: SerializeTupleStruct> SerializeTupleStruct for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), C::Error> {
        self.0.serialize_field(&Camel(v))
    }
    fn end(self) -> Result<C::Ok, C::Error> { self.0.end() }
}

impl<C: SerializeTupleVariant> SerializeTupleVariant for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), C::Error> {
        self.0.serialize_field(&Camel(v))
    }
    fn end(self) -> Result<C::Ok, C::Error> { self.0.end() }
}

impl<C: SerializeMap> SerializeMap for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;
    /// Map keys are data (file paths, extensions): passed through verbatim.
    fn serialize_key<T: ?Sized + Serialize>(&mut self, k: &T) -> Result<(), C::Error> {
        self.0.serialize_key(k)
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), C::Error> {
        self.0.serialize_value(&Camel(v))
    }
    fn end(self) -> Result<C::Ok, C::Error> { self.0.end() }
}

impl<C: SerializeStruct> SerializeStruct for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), C::Error> {
        self.0.serialize_field(camel_key(key), &Camel(v))
    }
    fn skip_field(&mut self, key: &'static str) -> Result<(), C::Error> {
        self.0.skip_field(camel_key(key))
    }
    fn end(self) -> Result<C::Ok, C::Error> { self.0.end() }
}

impl<C: SerializeStructVariant> SerializeStructVariant for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), C::Error> {
        self.0.serialize_field(camel_key(key), &Camel(v))
    }
    fn skip_field(&mut self, key: &'static str) -> Result<(), C::Error> {
        self.0.skip_field(camel_key(key))
    }
    fn end(self) -> Result<C::Ok, C::Error> { self.0.end() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(serde::Serialize)]
    struct Inner {
        module_id: String,
        is_bridge: Option<bool>,
    }

    #[derive(serde::Serialize)]
    enum Kind {
        SkipCall,
        Detail { source_path: String },
    }

    #[derive(serde::Serialize)]
    struct Outer {
        god_modules: Vec<Inner>,
        languages: BTreeMap<String, usize>,
        already_camel_ok: bool,
        #[serde(rename = "violationType")]
        violation_type: String,
        kind: Kind,
        detail: Kind,
    }

    #[test]
    fn renames_struct_fields_but_not_map_keys_or_values() {
        let mut languages = BTreeMap::new();
        languages.insert("src/foo_bar.rs".to_string(), 3);
        languages.insert("snake_key".to_string(), 1);
        let v = camel(&Outer {
            god_modules: vec![Inner { module_id: "a_b".into(), is_bridge: Some(true) }],
            languages,
            already_camel_ok: true,
            violation_type: "skip_call".into(),
            kind: Kind::SkipCall,
            detail: Kind::Detail { source_path: "x_y".into() },
        });
        assert_eq!(v["godModules"][0]["moduleId"], "a_b");
        assert_eq!(v["godModules"][0]["isBridge"], true);
        assert_eq!(v["languages"]["src/foo_bar.rs"], 3);
        assert_eq!(v["languages"]["snake_key"], 1);
        assert_eq!(v["alreadyCamelOk"], true);
        assert_eq!(v["violationType"], "skip_call");
        assert_eq!(v["kind"], "SkipCall");
        assert_eq!(v["detail"]["Detail"]["sourcePath"], "x_y");
    }

    #[test]
    fn json_values_pass_through() {
        let v = serde_json::json!({"snake_key": {"inner_key": 1}});
        assert_eq!(camel(&v), v);
    }

    #[test]
    fn camel_case_conversion() {
        assert_eq!(to_camel_case("layer_violation_count"), "layerViolationCount");
        assert_eq!(to_camel_case("path"), "path");
        assert_eq!(to_camel_case("fileA"), "fileA");
        assert_eq!(to_camel_case("_private_x"), "_privateX");
    }
}
