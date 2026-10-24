// Copyright 2026 AsterSQL.
use crate::json::go_object;
use astersql_pd_core::StoreInfo;
use serde::{Deserialize, Serialize};
/// Supported matching operators; unknown JSON strings remain invalid values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum LabelConstraintOp {
    In,
    NotIn,
    Exists,
    NotExists,
    Unknown(String),
}
impl Default for LabelConstraintOp {
    fn default() -> Self {
        Self::Unknown(String::new())
    }
}
impl From<String> for LabelConstraintOp {
    fn from(v: String) -> Self {
        match v.as_str() {
            "in" => Self::In,
            "notIn" => Self::NotIn,
            "exists" => Self::Exists,
            "notExists" => Self::NotExists,
            _ => Self::Unknown(v),
        }
    }
}
impl From<LabelConstraintOp> for String {
    fn from(v: LabelConstraintOp) -> Self {
        match v {
            LabelConstraintOp::In => "in".into(),
            LabelConstraintOp::NotIn => "notIn".into(),
            LabelConstraintOp::Exists => "exists".into(),
            LabelConstraintOp::NotExists => "notExists".into(),
            LabelConstraintOp::Unknown(v) => v,
        }
    }
}
impl LabelConstraintOp {
    pub fn is_valid(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
    fn is_empty(&self) -> bool {
        matches!(self,Self::Unknown(v) if v.is_empty())
    }
}
/// A conjunction member selecting a store label value.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LabelConstraint {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    #[serde(skip_serializing_if = "LabelConstraintOp::is_empty")]
    pub op: LabelConstraintOp,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}
go_object!(LabelConstraint,"key"=>key,"op"=>op,"values"=>values);
impl LabelConstraint {
    pub fn match_store(&self, store: &StoreInfo) -> bool {
        self.match_value(store.label_value(&self.key))
    }
    pub fn match_value(&self, value: &str) -> bool {
        match self.op {
            LabelConstraintOp::In => !value.is_empty() && self.values.iter().any(|v| v == value),
            LabelConstraintOp::NotIn => value.is_empty() || self.values.iter().all(|v| v != value),
            LabelConstraintOp::Exists => !value.is_empty(),
            LabelConstraintOp::NotExists => value.is_empty(),
            _ => false,
        }
    }
}
/// Exclusive keys must appear literally in the constraints, even with empty values.
pub fn is_exclusive_label(key: &str) -> bool {
    key.starts_with('$') || key == "engine" || key == "exclusive"
}
/// Nil stores never match. All constraints and exclusive-label gates must pass.
pub fn match_label_constraints(store: Option<&StoreInfo>, constraints: &[LabelConstraint]) -> bool {
    let Some(store) = store else { return false };
    if store
        .labels()
        .iter()
        .any(|l| is_exclusive_label(&l.key) && !constraints.iter().any(|c| c.key == l.key))
    {
        return false;
    }
    constraints.iter().all(|c| c.match_store(store))
}
