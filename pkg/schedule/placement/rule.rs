// Copyright 2026 AsterSQL.
use crate::{
    LabelConstraint,
    json::{go_object, json_string, zero},
};
use serde::{Deserialize, Serialize};
use std::fmt;
/// Expected placement peer role. Unknown input is retained for later validation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum PeerRoleType {
    Voter,
    Leader,
    Follower,
    Learner,
    Unknown(String),
}
impl Default for PeerRoleType {
    fn default() -> Self {
        Self::Unknown(String::new())
    }
}
impl From<String> for PeerRoleType {
    fn from(v: String) -> Self {
        match v.as_str() {
            "voter" => Self::Voter,
            "leader" => Self::Leader,
            "follower" => Self::Follower,
            "learner" => Self::Learner,
            _ => Self::Unknown(v),
        }
    }
}
impl From<PeerRoleType> for String {
    fn from(v: PeerRoleType) -> Self {
        match v {
            PeerRoleType::Voter => "voter".into(),
            PeerRoleType::Leader => "leader".into(),
            PeerRoleType::Follower => "follower".into(),
            PeerRoleType::Learner => "learner".into(),
            PeerRoleType::Unknown(v) => v,
        }
    }
}
impl PeerRoleType {
    pub fn is_valid(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}
/// Placement rule; decoded range bytes and group metadata are runtime only.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Rule {
    pub group_id: String,
    pub id: String,
    #[serde(skip_serializing_if = "zero")]
    pub index: i64,
    #[serde(rename = "override", skip_serializing_if = "zero")]
    pub override_: bool,
    #[serde(skip)]
    pub start_key: Vec<u8>,
    #[serde(rename = "start_key")]
    pub start_key_hex: String,
    #[serde(skip)]
    pub end_key: Vec<u8>,
    #[serde(rename = "end_key")]
    pub end_key_hex: String,
    pub role: PeerRoleType,
    pub is_witness: bool,
    pub count: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub label_constraints: Vec<LabelConstraint>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub location_labels: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub isolation_level: String,
    #[serde(skip_serializing_if = "zero")]
    pub version: u64,
    #[serde(skip_serializing_if = "zero")]
    pub create_timestamp: u64,
    #[serde(skip)]
    pub group: Option<RuleGroup>,
}
go_object!(Rule,"group_id"=>group_id,"id"=>id,"index"=>index,"override"=>override_,"start_key"=>start_key_hex,"end_key"=>end_key_hex,"role"=>role,"is_witness"=>is_witness,"count"=>count,"label_constraints"=>label_constraints,"location_labels"=>location_labels,"isolation_level"=>isolation_level,"version"=>version,"create_timestamp"=>create_timestamp);
impl Clone for Rule {
    fn clone(&self) -> Self {
        let mut r = Self::from_json(self.to_string().as_bytes()).expect("serialized rule");
        r.start_key = self.start_key.clone();
        r.end_key = self.end_key.clone();
        r
    }
}
impl Rule {
    pub fn from_json(data: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(data)
    }
    pub fn key(&self) -> [String; 2] {
        [self.group_id.clone(), self.id.clone()]
    }
    pub fn store_key(&self) -> String {
        fn hex(v: &str) -> String {
            v.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
        }
        format!("{}-{}", hex(&self.group_id), hex(&self.id))
    }
    pub fn group_index(&self) -> i64 {
        self.group.as_ref().map_or(0, |g| g.index)
    }
}
impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&json_string(self))
    }
}
/// Rule group priority and override properties.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RuleGroup {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(skip_serializing_if = "zero")]
    pub index: i64,
    #[serde(rename = "override", skip_serializing_if = "zero")]
    pub override_: bool,
}
go_object!(RuleGroup,"id"=>id,"index"=>index,"override"=>override_);
impl RuleGroup {
    pub fn from_json(data: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(data)
    }
    pub fn is_default(&self) -> bool {
        self.index == 0 && !self.override_
    }
}
impl fmt::Display for RuleGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&json_string(self))
    }
}
/// Group configuration and nullable rule list, preserving Go nil JSON values.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GroupBundle {
    #[serde(rename = "group_id")]
    pub id: String,
    #[serde(rename = "group_index")]
    pub index: i64,
    #[serde(rename = "group_override")]
    pub override_: bool,
    pub rules: Option<Vec<Option<Rule>>>,
}
go_object!(GroupBundle,"group_id"=>id,"group_index"=>index,"group_override"=>override_,"rules"=>rules);
impl fmt::Display for GroupBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&json_string(self))
    }
}
