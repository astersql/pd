// Copyright 2026 AsterSQL.
#[test]
fn sa_011_binding_preserves_scalar_on_duplicate_null() {
    let value = super::accept_http_10::bind(
        br#"{"new-id":4097,"new-id":null}"#,
        "SplitKeyspaceGroupByIDParams",
    )
    .unwrap();
    assert_eq!(value["new-id"], 4097);
    let value = super::accept_http_10::bind(
        br#"{"merge-all-into-default":true,"merge-all-into-default":null}"#,
        "MergeKeyspaceGroupsParams",
    )
    .unwrap();
    assert_eq!(value["merge-all-into-default"], true);
}
#[test]
fn sa_011_binding_numeric_string_element() {
    let error = super::accept_http_10::bind(br#"{"nodes":[42]}"#, "SetNodesForKeyspaceGroupParams")
        .unwrap_err();
    assert!(error.1.as_str().unwrap().contains("cannot unmarshal number into Go struct field SetNodesForKeyspaceGroupParams.nodes of type string"),"{error:?}");
}
#[test]
fn sa_011_binding_priority_overflow() {
    let error = super::accept_http_10::bind(
        br#"{"priority":18446744073709551615}"#,
        "SetPriorityForKeyspaceGroupParams",
    )
    .unwrap_err();
    assert!(error.1.as_str().unwrap().contains("cannot unmarshal number 18446744073709551615 into Go struct field SetPriorityForKeyspaceGroupParams.priority of type int"),"{error:?}");
}
