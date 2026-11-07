// Copyright 2026 AsterSQL.

use super::candidate::Candidate;

#[test]
fn theme10_candidate_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meta.db");
    let store = Candidate::open(&path).unwrap();
    assert_eq!(rusqlite::version(), "3.51.3");
    #[cfg(target_os = "macos")]
    assert!(
        store
            .db
            .pragma_query_value(None, "fullfsync", |row| row.get::<_, bool>(0))
            .unwrap()
    );
    store
        .db
        .execute(
            "INSERT INTO state(name, payload) VALUES ('probe', ?)",
            [b"persisted"],
        )
        .unwrap();
    store.db.close().unwrap();

    let reopened = Candidate::open(&path).unwrap();
    let data: Vec<u8> = reopened
        .db
        .query_row(
            "SELECT payload FROM state WHERE name = 'probe'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(data, b"persisted");
}
