use lite_vote::{
    db::{DatabaseConfig, MIGRATOR, connect, connect_pool, validate_migrations},
    models::{
        Choice, Participant, Vote, VotingRoom, find_choice, find_participant, find_vote,
        find_voting_room, insert_choice, insert_participant, insert_vote, insert_voting_room,
        replace_choices, update_vote_choice,
    },
};
use sqlx::Executor;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn config(busy_timeout: Duration) -> (tempfile::TempDir, DatabaseConfig) {
    let dir = tempdir().unwrap();
    let config = DatabaseConfig {
        path: dir.path().join("test.sqlite3"),
        busy_timeout,
    };
    (dir, config)
}

async fn migrated_pool(config: &DatabaseConfig) -> sqlx::SqlitePool {
    let pool = connect_pool(config).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    pool
}

async fn seed(pool: &sqlx::SqlitePool) {
    let mut tx = pool.begin().await.unwrap();
    insert_voting_room(
        &mut tx,
        &VotingRoom {
            id: 1,
            slug: "one".into(),
            question: "Q".into(),
            participant_names_public: false,
            creator_token_hash: "creator".into(),
            created_at: "now".into(),
            closed_at: None,
        },
    )
    .await
    .unwrap();
    insert_choice(
        &mut tx,
        &Choice {
            room_id: 1,
            id: 1,
            text: " A ".into(),
            position: 0,
        },
    )
    .await
    .unwrap();
    insert_choice(
        &mut tx,
        &Choice {
            room_id: 1,
            id: 2,
            text: "B".into(),
            position: 1,
        },
    )
    .await
    .unwrap();
    insert_participant(
        &mut tx,
        &Participant {
            room_id: 1,
            id: 1,
            token_hash: "p1".into(),
            display_name: Some("Alice".into()),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn migration_is_idempotent_models_persist_and_reconnect() {
    let (_dir, config) = config(Duration::from_secs(5));
    let pool = migrated_pool(&config).await;
    MIGRATOR.run(&pool).await.unwrap();
    seed(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    insert_vote(
        &mut tx,
        &Vote {
            room_id: 1,
            participant_id: 1,
            choice_id: 1,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    pool.close().await;

    let database = connect(config).await.unwrap();
    assert_eq!(
        find_voting_room(&database.pool, 1)
            .await
            .unwrap()
            .unwrap()
            .question,
        "Q"
    );
    assert_eq!(
        find_choice(&database.pool, 1, 1)
            .await
            .unwrap()
            .unwrap()
            .text,
        "A"
    );
    assert_eq!(
        find_participant(&database.pool, 1, 1)
            .await
            .unwrap()
            .unwrap()
            .display_name
            .as_deref(),
        Some("Alice")
    );
    assert_eq!(
        find_vote(&database.pool, 1, 1)
            .await
            .unwrap()
            .unwrap()
            .choice_id,
        1
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&database.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn vote_constraints_and_update_are_enforced() {
    let (_dir, config) = config(Duration::from_secs(5));
    let pool = migrated_pool(&config).await;
    seed(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    insert_vote(
        &mut tx,
        &Vote {
            room_id: 1,
            participant_id: 1,
            choice_id: 1,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    assert!(
        insert_vote(
            &mut tx,
            &Vote {
                room_id: 1,
                participant_id: 1,
                choice_id: 2
            }
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(update_vote_choice(&mut tx, 1, 1, 2).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(find_vote(&pool, 1, 1).await.unwrap().unwrap().choice_id, 2);

    let mut tx = pool.begin().await.unwrap();
    insert_voting_room(
        &mut tx,
        &VotingRoom {
            id: 2,
            slug: "two".into(),
            question: "Q2".into(),
            participant_names_public: false,
            creator_token_hash: "creator2".into(),
            created_at: "now".into(),
            closed_at: None,
        },
    )
    .await
    .unwrap();
    insert_choice(
        &mut tx,
        &Choice {
            room_id: 2,
            id: 3,
            text: "C".into(),
            position: 0,
        },
    )
    .await
    .unwrap();
    insert_participant(
        &mut tx,
        &Participant {
            room_id: 1,
            id: 2,
            token_hash: "p2".into(),
            display_name: None,
        },
    )
    .await
    .unwrap();
    insert_participant(
        &mut tx,
        &Participant {
            room_id: 2,
            id: 3,
            token_hash: "p3".into(),
            display_name: None,
        },
    )
    .await
    .unwrap();
    assert!(
        insert_vote(
            &mut tx,
            &Vote {
                room_id: 1,
                participant_id: 2,
                choice_id: 3
            }
        )
        .await
        .is_err()
    );
    assert!(
        insert_vote(
            &mut tx,
            &Vote {
                room_id: 1,
                participant_id: 3,
                choice_id: 1,
            }
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn choices_can_only_be_replaced_before_voting_starts() {
    let (_dir, config) = config(Duration::from_secs(5));
    let pool = migrated_pool(&config).await;
    seed(&pool).await;

    let replacements = [
        Choice {
            room_id: 1,
            id: 10,
            text: " New first ".into(),
            position: 0,
        },
        Choice {
            room_id: 1,
            id: 11,
            text: "New second".into(),
            position: 1,
        },
    ];
    let mut tx = pool.begin().await.unwrap();
    replace_choices(&mut tx, 1, &replacements).await.unwrap();
    tx.commit().await.unwrap();

    let stored: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT id, text, position FROM choices WHERE room_id = 1 ORDER BY position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored,
        vec![(10, "New first".into(), 0), (11, "New second".into(), 1)]
    );

    let mut tx = pool.begin().await.unwrap();
    insert_vote(
        &mut tx,
        &Vote {
            room_id: 1,
            participant_id: 1,
            choice_id: 10,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    assert!(replace_choices(&mut tx, 1, &replacements).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(find_vote(&pool, 1, 1).await.unwrap().unwrap().choice_id, 10);
}

#[tokio::test]
async fn uniqueness_foreign_keys_pragmas_and_closed_state_are_enforced() {
    let (_dir, config) = config(Duration::from_millis(1_234));
    let pool = migrated_pool(&config).await;
    seed(&pool).await;

    for statement in [
        "INSERT INTO choices VALUES(1,3,'B',2)",
        "INSERT INTO choices VALUES(1,3,'C',1)",
        "INSERT INTO participants VALUES(1,2,'p1',NULL)",
        "INSERT INTO participants VALUES(99,1,'x',NULL)",
    ] {
        assert!(sqlx::query(statement).execute(&pool).await.is_err());
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .unwrap()
            .to_lowercase(),
        "wal"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA busy_timeout")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1_234
    );
    assert!(
        !find_voting_room(&pool, 1)
            .await
            .unwrap()
            .unwrap()
            .is_closed()
    );
    sqlx::query("UPDATE voting_rooms SET closed_at = 'later' WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        find_voting_room(&pool, 1)
            .await
            .unwrap()
            .unwrap()
            .is_closed()
    );
}

#[tokio::test]
async fn busy_timeout_waits_for_another_writer() {
    let (_dir, config) = config(Duration::from_millis(500));
    let pool = migrated_pool(&config).await;
    seed(&pool).await;

    let mut first = pool.acquire().await.unwrap();
    first.execute("BEGIN IMMEDIATE").await.unwrap();
    let second_pool = connect_pool(&config).await.unwrap();
    let started = Instant::now();
    let result = sqlx::query("UPDATE voting_rooms SET question = 'blocked' WHERE id = 1")
        .execute(&second_pool)
        .await;
    assert!(result.is_err());
    assert!(started.elapsed() >= Duration::from_millis(400));
    first.execute("ROLLBACK").await.unwrap();
}

#[tokio::test]
async fn startup_rejects_missing_and_mismatched_migrations() {
    let (_dir, config) = config(Duration::from_secs(5));
    let pool = connect_pool(&config).await.unwrap();
    assert!(validate_migrations(&pool).await.is_err());
    MIGRATOR.run(&pool).await.unwrap();
    validate_migrations(&pool).await.unwrap();
    sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(validate_migrations(&pool).await.is_err());
}

#[tokio::test]
async fn empty_database_can_be_migrated_by_sqlx() {
    let (_dir, config) = config(Duration::from_secs(5));
    let pool = connect_pool(&config).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    for table in ["voting_rooms", "choices", "participants", "votes"] {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(exists, 1, "{table}");
    }
}
