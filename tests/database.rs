use lite_vote::db::{DatabaseConfig, migrate, open_configured};
use rusqlite::{Connection, params};
use std::time::Duration;
use tempfile::tempdir;

fn config() -> (tempfile::TempDir, DatabaseConfig) {
    let dir = tempdir().unwrap();
    let config = DatabaseConfig {
        path: dir.path().join("test.sqlite3"),
        busy_timeout: Duration::from_secs(5),
    };
    (dir, config)
}
fn seed(conn: &Connection) {
    conn.execute(
        "INSERT INTO voting_rooms VALUES(1,'one','Q',0,'creator','now',NULL)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO choices VALUES(1,1,' A ',0)", [])
        .unwrap();
    conn.execute("INSERT INTO choices VALUES(1,2,'B',1)", [])
        .unwrap();
    conn.execute("INSERT INTO participants VALUES(1,1,'p1',NULL)", [])
        .unwrap();
}

#[test]
fn migration_is_idempotent_and_data_persists() {
    let (_dir, config) = config();
    migrate(&config).unwrap();
    migrate(&config).unwrap();
    {
        let conn = open_configured(&config).unwrap();
        seed(&conn);
    }
    let conn = open_configured(&config).unwrap();
    assert_eq!(
        conn.query_row("SELECT question FROM voting_rooms WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "Q"
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn vote_constraints_and_update_are_enforced() {
    let (_dir, config) = config();
    migrate(&config).unwrap();
    let conn = open_configured(&config).unwrap();
    seed(&conn);
    conn.execute("INSERT INTO votes VALUES(1,1,1)", []).unwrap();
    assert!(conn.execute("INSERT INTO votes VALUES(1,1,2)", []).is_err());
    conn.execute(
        "UPDATE votes SET choice_id=2 WHERE room_id=1 AND participant_id=1",
        [],
    )
    .unwrap();
    assert_eq!(
        conn.query_row("SELECT choice_id FROM votes", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );

    conn.execute(
        "INSERT INTO voting_rooms VALUES(2,'two','Q2',0,'creator2','now',NULL)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO choices VALUES(2,3,'C',0)", [])
        .unwrap();
    conn.execute("INSERT INTO participants VALUES(1,2,'p2',NULL)", [])
        .unwrap();
    assert!(conn.execute("INSERT INTO votes VALUES(1,2,3)", []).is_err());
}

#[test]
fn uniqueness_foreign_keys_and_pragmas_are_enforced() {
    let (_dir, config) = config();
    migrate(&config).unwrap();
    let conn = open_configured(&config).unwrap();
    seed(&conn);
    assert!(
        conn.execute("INSERT INTO choices VALUES(1,3,'B',2)", [])
            .is_err()
    );
    assert!(
        conn.execute("INSERT INTO choices VALUES(1,3,'C',1)", [])
            .is_err()
    );
    assert!(
        conn.execute("INSERT INTO participants VALUES(1,2,'p1',NULL)", [])
            .is_err()
    );
    assert!(
        conn.execute("INSERT INTO participants VALUES(99,1,'x',NULL)", [])
            .is_err()
    );
    assert_eq!(
        conn.pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
            .unwrap()
            .to_lowercase(),
        "wal"
    );
    assert_eq!(
        conn.pragma_query_value(None, "foreign_keys", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.pragma_query_value(None, "busy_timeout", |r| r.get::<_, i64>(0))
            .unwrap(),
        5000
    );
}

#[test]
fn busy_timeout_waits_for_another_writer() {
    let (_dir, config) = config();
    migrate(&config).unwrap();
    let first = open_configured(&config).unwrap();
    seed(&first);
    first.execute_batch("BEGIN IMMEDIATE").unwrap();
    let second = open_configured(&config).unwrap();
    let started = std::time::Instant::now();
    let result = second.execute(
        "UPDATE voting_rooms SET question='blocked' WHERE id=1",
        params![],
    );
    assert!(result.is_err());
    assert!(started.elapsed() >= Duration::from_millis(4500));
    first.execute_batch("ROLLBACK").unwrap();
}
