// Copyright 2026 AsterSQL.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

/// Numeric user kind; unknown Go enum values remain representable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserKind(pub i64);

impl UserKind {
    pub const BASIC: Self = Self(0);
    pub const STANDARD: Self = Self(1);
    pub const ENTERPRISE: Self = Self(2);
    pub const COUNT: Self = Self(3);

    /// Converts the exact lowercase Go name, defaulting unknown inputs to basic.
    pub fn from_name(input: &str) -> Self {
        match input {
            "standard" => Self::STANDARD,
            "enterprise" => Self::ENTERPRISE,
            _ => Self::BASIC,
        }
    }

    /// Reports whether the input is one of the three exact Go names.
    pub fn is_valid(input: &str) -> bool {
        matches!(input, "basic" | "standard" | "enterprise")
    }
}

impl fmt::Display for UserKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            Self::BASIC => "basic",
            Self::STANDARD => "standard",
            Self::ENTERPRISE => "enterprise",
            _ => "unknown UserKind",
        })
    }
}

/// Election member address and signed priority, matching the Go JSON fields.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct KeyspaceGroupMember {
    pub address: String,
    pub priority: i64,
}

impl KeyspaceGroupMember {
    /// Compares member addresses using the Go base URL equivalence rule.
    pub fn is_address_equivalent(&self, address: &str) -> bool {
        // Go TrimScheme removes https first, then http, even for nested prefixes.
        fn trim(s: &str) -> &str {
            let s = s.strip_prefix("https://").unwrap_or(s);
            s.strip_prefix("http://").unwrap_or(s)
        }
        trim(&self.address) == trim(address)
    }
}

impl<'de> Deserialize<'de> for KeyspaceGroupMember {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MemberVisitor;
        impl<'de> de::Visitor<'de> for MemberVisitor {
            type Value = KeyspaceGroupMember;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a keyspace group member object or null")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(KeyspaceGroupMember::default())
            }
            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut member = KeyspaceGroupMember::default();
                while let Some(key) = map.next_key::<String>()? {
                    if key.eq_ignore_ascii_case("address") {
                        if let Some(address) = map.next_value::<Option<String>>()? {
                            member.address = address;
                        }
                    } else if key.eq_ignore_ascii_case("priority") {
                        if let Some(priority) = map.next_value::<Option<i64>>()? {
                            member.priority = priority;
                        }
                    } else {
                        map.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(member)
            }
        }
        deserializer.deserialize_any(MemberVisitor)
    }
}
