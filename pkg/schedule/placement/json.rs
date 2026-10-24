// Copyright 2026 AsterSQL.
//! Go encoding/json object semantics: null leaves zero values, folded field
//! names and duplicate fields are applied in input order; unknown fields ignored.
macro_rules! go_field {
    ($m:ident,$result:ident,location_labels,$name:literal) => {$crate::json::go_field!(@strings $m,$result,location_labels)};
    ($m:ident,$result:ident,values,$name:literal) => {$crate::json::go_field!(@strings $m,$result,values)};
    (@strings $m:ident,$result:ident,$field:ident) => {{
        $result.$field=$m.next_value::<Option<Vec<Option<String>>>>()?.unwrap_or_default().into_iter().map(Option::unwrap_or_default).collect();
    }};
    ($m:ident,$result:ident,$field:ident,$name:literal) => {{
        if let Some(v)=$m.next_value::<Option<_>>()? {$result.$field=v;}
        else if matches!($name,"label_constraints"|"rules") {$result.$field=Default::default();}
    }};
}
pub(crate) use go_field;
macro_rules! go_object {
    ($ty:ty, $($name:literal => $field:ident),* $(,)?) => {
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self,D::Error> {
                struct Visitor;
                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = $ty;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {f.write_str("object or null")}
                    fn visit_unit<E: serde::de::Error>(self)->Result<Self::Value,E>{Ok(<$ty>::default())}
                    fn visit_map<M: serde::de::MapAccess<'de>>(self,mut m:M)->Result<Self::Value,M::Error>{
                        let mut result=<$ty>::default();
                        while let Some(key)=m.next_key::<String>()? {
                            match $crate::json::fold_field(&key).as_str() {
                                $($name => {$crate::json::go_field!(m,result,$field,$name);},)*
                                _ => {m.next_value::<serde::de::IgnoredAny>()?;}
                            }
                        }
                        Ok(result)
                    }
                }
                d.deserialize_any(Visitor)
            }
        }
    }
}
pub(crate) use go_object;
pub(crate) fn zero<T: Default + PartialEq>(v: &T) -> bool {
    v == &T::default()
}
pub(crate) fn json_string<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v)
        .expect("rule values serialize")
        .replace('&', "\u{005c}u0026")
        .replace('<', "\u{005c}u003c")
        .replace('>', "\u{005c}u003e")
        .replace('\u{2028}', "\u{005c}u2028")
        .replace('\u{2029}', "\u{005c}u2029")
}

// Go encoding/json foldName includes the long-s and Kelvin simple-fold classes.
pub(crate) fn fold_field(key: &str) -> String {
    key.chars()
        .map(|c| match c {
            'ſ' => 's',
            'K' => 'k',
            _ => c.to_ascii_lowercase(),
        })
        .collect()
}
