# label_constraint.rs

公开 LabelConstraintOp::{In,NotIn,Exists,NotExists,Unknown,is_valid}；LabelConstraint::{key,op,values,match_store,match_value}；is_exclusive_label、match_label_constraints。空值等于缺失，未知 op 返回 false。复用 core::StoreInfo 首个折叠键查找；集合匹配拒绝 None，独占键精确匹配再做合取。无 I/O。

测试与 Go oracle 见 [批次 21](../../../docs/rust/schedule/placement/21-证据.md)。
