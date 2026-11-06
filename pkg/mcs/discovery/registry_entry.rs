// Copyright 2026 AsterSQL.
use serde::Serialize;
/// The six fields persisted by Go ServiceRegistryEntry; empty fields are retained.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ServiceRegistryEntry {
    pub name: String,
    #[serde(rename = "service-addr")]
    pub service_addr: String,
    pub version: String,
    #[serde(rename = "git-hash")]
    pub git_hash: String,
    #[serde(rename = "deploy-path")]
    pub deploy_path: String,
    #[serde(rename = "start-timestamp")]
    pub start_timestamp: i64,
}
impl ServiceRegistryEntry {
    pub fn serialize(&self) -> Result<Vec<u8>, serde_json::Error> {
        let encoded = serde_json::to_string(self)?;
        Ok(encoded
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029")
            .into_bytes())
    }
    /// Follow Go's case-insensitive field matching, null no-op and repeated-field order.
    pub fn deserialize(bytes: &[u8]) -> Result<Self, String> {
        use serde::Deserialize;
        struct Fields(Vec<(String, serde_json::Value)>);
        impl<'de> Deserialize<'de> for Fields {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct Visitor;
                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = Fields;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("registry object")
                    }
                    fn visit_unit<E: serde::de::Error>(self) -> Result<Fields, E> {
                        Ok(Fields(vec![]))
                    }
                    fn visit_map<M: serde::de::MapAccess<'de>>(
                        self,
                        mut m: M,
                    ) -> Result<Fields, M::Error> {
                        let mut v = vec![];
                        while let Some(e) = m.next_entry()? {
                            v.push(e);
                        }
                        Ok(Fields(v))
                    }
                }
                d.deserialize_any(Visitor)
            }
        }
        let text = String::from_utf8_lossy(bytes);
        let fields: Fields = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let mut e = Self::default();
        for (key, v) in fields.0 {
            if v.is_null() {
                continue;
            }
            let target = match key.to_ascii_lowercase().as_str() {
                "name" => Some(&mut e.name),
                "service-addr" => Some(&mut e.service_addr),
                "version" => Some(&mut e.version),
                "git-hash" => Some(&mut e.git_hash),
                "deploy-path" => Some(&mut e.deploy_path),
                "start-timestamp" => {
                    e.start_timestamp = v.as_i64().ok_or("invalid start-timestamp")?;
                    None
                }
                _ => None,
            };
            if let Some(t) = target {
                *t = v.as_str().ok_or("invalid registry string")?.into();
            }
        }
        Ok(e)
    }
}
