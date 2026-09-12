// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use std::env;
use std::time::Duration;

use product_application::{
    ChangeReadingEntryState, ChangeReadingEntryStateAndRecordProgress,
    ChangeReadingEntryStateCommand, ChangeReadingEntryStateError, CreateReadingEntry,
    CreateReadingEntryCommand, CreateReadingEntryError, GetReadingProgress, ListReadingEntries,
    ListReadingEntriesError, ListReadingEntriesQuery, ReadingQueueEntryState,
};
use product_persistence_postgres::{Database, apply_migrations};

const CURSOR_SIGNING_KEY: &[u8] = b"0123456789abcdef0123456789abcdef";

fn all_entries_query() -> ListReadingEntriesQuery {
    ListReadingEntriesQuery {
        status: None,
        sort: None,
        limit: Some(50),
        cursor: None,
        authorization_scope: "anonymous".to_owned(),
    }
}

#[tokio::test]
#[ignore = "requires an isolated migrated PostgreSQL database prepared explicitly for integration tests"]
async fn applied_migration_history_rejects_mutation_and_deletion() {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL for a disposable test database");
    apply_migrations(&database_url)
        .await
        .expect("apply compiled migrations");
    let database = Database::connect(&database_url, 2)
        .await
        .expect("connect to PostgreSQL");
    database
        .verify_compiled_migrations()
        .await
        .expect("fresh applied history is compatible");

    let mut fixture = database.begin().await.expect("begin migration fixture");
    sqlx::query(
        "CREATE TABLE yydra_migration_fixture AS SELECT * FROM _sqlx_migrations WITH NO DATA",
    )
    .execute(&mut *fixture)
    .await
    .expect("create migration backup table");
    sqlx::query(
        "INSERT INTO yydra_migration_fixture SELECT * FROM _sqlx_migrations WHERE version = 5",
    )
    .execute(&mut *fixture)
    .await
    .expect("back up latest migration row");
    fixture.commit().await.expect("commit migration fixture");

    let mut mutation = database.begin().await.expect("begin checksum mutation");
    sqlx::query("UPDATE _sqlx_migrations SET checksum = decode('00', 'hex') WHERE version = 5")
        .execute(&mut *mutation)
        .await
        .expect("mutate applied checksum");
    mutation.commit().await.expect("commit checksum mutation");
    assert!(
        database.verify_compiled_migrations().await.is_err(),
        "an edited applied migration must fail closed"
    );
    let mut restore = database.begin().await.expect("begin checksum restore");
    sqlx::query(
        "UPDATE _sqlx_migrations AS applied SET checksum = fixture.checksum FROM yydra_migration_fixture AS fixture WHERE applied.version = fixture.version",
    )
    .execute(&mut *restore)
    .await
    .expect("restore applied checksum");
    restore.commit().await.expect("commit checksum restore");

    let mut deletion = database.begin().await.expect("begin applied deletion");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 5")
        .execute(&mut *deletion)
        .await
        .expect("delete applied migration");
    deletion.commit().await.expect("commit applied deletion");
    assert!(
        database.verify_compiled_migrations().await.is_err(),
        "a deleted applied migration must fail closed"
    );
    let mut cleanup = database.begin().await.expect("begin migration restore");
    sqlx::query("INSERT INTO _sqlx_migrations SELECT * FROM yydra_migration_fixture")
        .execute(&mut *cleanup)
        .await
        .expect("restore deleted migration");
    sqlx::query("DROP TABLE yydra_migration_fixture")
        .execute(&mut *cleanup)
        .await
        .expect("drop migration backup table");
    cleanup.commit().await.expect("commit migration restore");
    database
        .verify_compiled_migrations()
        .await
        .expect("restored applied history is compatible");
}

#[tokio::test]
#[ignore = "requires an isolated migrated PostgreSQL database prepared explicitly for integration tests"]
async fn reading_queue_use_cases_commit_success_and_rollback_failures() {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL for a disposable test database");
    apply_migrations(&database_url)
        .await
        .expect("apply compiled migrations");
    let database = Database::connect(&database_url, 2)
        .await
        .expect("connect to PostgreSQL");
    let mut cleanup = database.begin().await.expect("begin cleanup transaction");
    sqlx::query("DELETE FROM reading_queue_entries")
        .execute(&mut *cleanup)
        .await
        .expect("clear reading queue fixture");
    cleanup.commit().await.expect("commit fixture cleanup");

    let create = CreateReadingEntry::new(database.clone());
    let change = ChangeReadingEntryState::new(database.clone());
    let list = ListReadingEntries::new(database.clone(), CURSOR_SIGNING_KEY)
        .expect("valid cursor signing key");
    let created = create
        .execute(CreateReadingEntryCommand {
            title: "PostgreSQL transactions in practice".to_owned(),
            source_url: "https://example.test/postgres-transactions".to_owned(),
        })
        .await
        .expect("commit a valid reading entry");
    assert!(!created.id.is_empty());
    assert_eq!(created.state, ReadingQueueEntryState::Queued);
    assert_eq!(
        list.execute(all_entries_query())
            .await
            .expect("list committed entry")
            .entries,
        vec![created.clone()]
    );

    let completed = change
        .execute(ChangeReadingEntryStateCommand {
            id: created.id.clone(),
            target: ReadingQueueEntryState::Completed,
        })
        .await
        .expect("complete a queued entry");
    assert_eq!(completed.state, ReadingQueueEntryState::Completed);
    let conflict = change
        .execute(ChangeReadingEntryStateCommand {
            id: created.id.clone(),
            target: ReadingQueueEntryState::Completed,
        })
        .await
        .expect_err("completing twice must conflict");
    assert!(matches!(
        conflict,
        ChangeReadingEntryStateError::Conflict { .. }
    ));
    assert_eq!(
        list.execute(all_entries_query())
            .await
            .expect("conflict leaves committed state")
            .entries[0]
            .state,
        ReadingQueueEntryState::Completed
    );
    let reopened = change
        .execute(ChangeReadingEntryStateCommand {
            id: created.id.clone(),
            target: ReadingQueueEntryState::Queued,
        })
        .await
        .expect("reopen a completed entry");
    assert_eq!(reopened.state, ReadingQueueEntryState::Queued);
    let missing = change
        .execute(ChangeReadingEntryStateCommand {
            id: "missing-opaque-entry".to_owned(),
            target: ReadingQueueEntryState::Completed,
        })
        .await
        .expect_err("missing entry must not be created by transition");
    assert!(matches!(
        missing,
        ChangeReadingEntryStateError::NotFound { .. }
    ));

    let invalid = create
        .execute(CreateReadingEntryCommand {
            title: "   ".to_owned(),
            source_url: "https://example.test/not-inserted".to_owned(),
        })
        .await
        .expect_err("Domain validation must fail before insertion");
    assert!(matches!(
        invalid,
        CreateReadingEntryError::InvalidInput { .. }
    ));
    assert_eq!(
        list.execute(all_entries_query())
            .await
            .expect("list after Domain rejection")
            .entries
            .len(),
        1
    );

    let mut failed = database
        .begin()
        .await
        .expect("begin failed command fixture");
    sqlx::query(
        "INSERT INTO reading_queue_entries (title, source_url, state) VALUES ($1, $2, 'queued')",
    )
    .bind("temporarily inserted")
    .bind("https://example.test/temporary")
    .execute(&mut *failed)
    .await
    .expect("first statement succeeds inside the transaction");
    let constraint_failure = sqlx::query(
        "INSERT INTO reading_queue_entries (title, source_url, state) VALUES ($1, $2, $3)",
    )
    .bind("")
    .bind("not-an-http-url")
    .bind("invented-state")
    .execute(&mut *failed)
    .await;
    assert!(
        constraint_failure.is_err(),
        "database constraints must reject invalid rows"
    );
    failed
        .rollback()
        .await
        .expect("roll back the failed command");
    assert_eq!(
        list.execute(all_entries_query())
            .await
            .expect("list after transaction rollback")
            .entries
            .len(),
        1,
        "the earlier statement in the failed command must not leak"
    );

    let mut cleanup = database.begin().await.expect("begin final cleanup");
    sqlx::query("DELETE FROM reading_queue_entries")
        .execute(&mut *cleanup)
        .await
        .expect("remove transaction fixture entry");
    cleanup.commit().await.expect("commit final cleanup");
}

#[tokio::test]
#[ignore = "requires an isolated migrated PostgreSQL database prepared explicitly for integration tests"]
async fn cross_domain_orchestration_keeps_progress_synchronous_and_rolls_back_together() {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL for a disposable test database");
    apply_migrations(&database_url)
        .await
        .expect("apply compiled migrations");
    let database = Database::connect(&database_url, 4)
        .await
        .expect("connect to PostgreSQL");
    let mut fixture = database.begin().await.expect("begin fixture reset");
    sqlx::query("DELETE FROM reading_queue_entries")
        .execute(&mut *fixture)
        .await
        .expect("clear reading entries");
    sqlx::query("UPDATE reading_progress SET completed_entries = 0 WHERE singleton")
        .execute(&mut *fixture)
        .await
        .expect("reset reading progress");
    fixture.commit().await.expect("commit fixture reset");

    let create = CreateReadingEntry::new(database.clone());
    let change = ChangeReadingEntryStateAndRecordProgress::new(database.clone());
    let progress = GetReadingProgress::new(database.clone());
    let list = ListReadingEntries::new(database.clone(), CURSOR_SIGNING_KEY)
        .expect("valid cursor signing key");
    assert_eq!(
        progress
            .execute()
            .await
            .expect("initial progress")
            .completed_entries,
        0
    );
    let entry = create
        .execute(CreateReadingEntryCommand {
            title: "Transactional orchestration".to_owned(),
            source_url: "https://example.test/orchestration".to_owned(),
        })
        .await
        .expect("create queued entry");
    change
        .execute(ChangeReadingEntryStateCommand {
            id: entry.id.clone(),
            target: ReadingQueueEntryState::Completed,
        })
        .await
        .expect("complete and record progress in one transaction");
    assert_eq!(
        progress
            .execute()
            .await
            .expect("completed progress")
            .completed_entries,
        1
    );
    change
        .execute(ChangeReadingEntryStateCommand {
            id: entry.id.clone(),
            target: ReadingQueueEntryState::Queued,
        })
        .await
        .expect("reopen and record progress in one transaction");
    assert_eq!(
        progress
            .execute()
            .await
            .expect("reopened progress")
            .completed_entries,
        0
    );

    let mut remove_progress = database.begin().await.expect("begin fault fixture");
    sqlx::query("DELETE FROM reading_progress")
        .execute(&mut *remove_progress)
        .await
        .expect("remove progress singleton");
    remove_progress
        .commit()
        .await
        .expect("commit fault fixture");
    let failure = change
        .execute(ChangeReadingEntryStateCommand {
            id: entry.id.clone(),
            target: ReadingQueueEntryState::Completed,
        })
        .await
        .expect_err("progress failure must reject the whole orchestration");
    assert!(matches!(failure, ChangeReadingEntryStateError::Storage(_)));
    assert_eq!(
        list.execute(all_entries_query())
            .await
            .expect("list after orchestration rollback")
            .entries[0]
            .state,
        ReadingQueueEntryState::Queued,
        "the Reading Queue transition must roll back with Reading Progress"
    );

    let mut cleanup = database.begin().await.expect("begin final cleanup");
    sqlx::query("DELETE FROM reading_queue_entries")
        .execute(&mut *cleanup)
        .await
        .expect("remove entry fixture");
    sqlx::query(
        "INSERT INTO reading_progress (singleton, completed_entries) VALUES (TRUE, 0) ON CONFLICT (singleton) DO UPDATE SET completed_entries = 0",
    )
    .execute(&mut *cleanup)
    .await
    .expect("restore progress singleton");
    cleanup.commit().await.expect("commit final cleanup");
}

#[tokio::test]
#[ignore = "requires an isolated migrated PostgreSQL database prepared explicitly for integration tests"]
async fn read_committed_row_lock_serializes_conflicting_commands_without_retry() {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL for a disposable test database");
    apply_migrations(&database_url)
        .await
        .expect("apply compiled migrations");
    let database = Database::connect(&database_url, 6)
        .await
        .expect("connect to PostgreSQL");
    let mut fixture = database.begin().await.expect("begin fixture reset");
    let isolation = sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *fixture)
        .await
        .expect("read transaction isolation");
    assert_eq!(isolation, "read committed");
    sqlx::query("DELETE FROM reading_queue_entries")
        .execute(&mut *fixture)
        .await
        .expect("clear reading entries");
    sqlx::query("UPDATE reading_progress SET completed_entries = 0 WHERE singleton")
        .execute(&mut *fixture)
        .await
        .expect("reset reading progress");
    fixture.commit().await.expect("commit fixture reset");

    let create = CreateReadingEntry::new(database.clone());
    let change = ChangeReadingEntryStateAndRecordProgress::new(database.clone());
    let progress = GetReadingProgress::new(database.clone());
    let entry = create
        .execute(CreateReadingEntryCommand {
            title: "Deterministic contention".to_owned(),
            source_url: "https://example.test/contention".to_owned(),
        })
        .await
        .expect("create contention entry");

    let mut blocker = database.begin().await.expect("begin row-lock fixture");
    sqlx::query("SELECT id FROM reading_queue_entries WHERE id::text = $1 FOR SHARE")
        .bind(&entry.id)
        .fetch_one(&mut *blocker)
        .await
        .expect("hold a shared lock that the demonstrated exclusive lock must wait behind");
    let blocked_change = change.clone();
    let blocked_id = entry.id.clone();
    let mut blocked = tokio::spawn(async move {
        blocked_change
            .execute(ChangeReadingEntryStateCommand {
                id: blocked_id,
                target: ReadingQueueEntryState::Completed,
            })
            .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut blocked)
            .await
            .is_err(),
        "the command must wait for the selected row lock"
    );
    blocker.rollback().await.expect("release row-lock fixture");
    blocked
        .await
        .expect("contention task must not panic")
        .expect("command completes after the row lock is released");
    change
        .execute(ChangeReadingEntryStateCommand {
            id: entry.id.clone(),
            target: ReadingQueueEntryState::Queued,
        })
        .await
        .expect("reopen before simultaneous commands");

    let first = change.execute(ChangeReadingEntryStateCommand {
        id: entry.id.clone(),
        target: ReadingQueueEntryState::Completed,
    });
    let second = change.execute(ChangeReadingEntryStateCommand {
        id: entry.id.clone(),
        target: ReadingQueueEntryState::Completed,
    });
    let results = tokio::join!(first, second);
    let successes = [&results.0, &results.1]
        .into_iter()
        .filter(|result| result.is_ok())
        .count();
    let conflicts = [results.0, results.1]
        .into_iter()
        .filter(|result| matches!(result, Err(ChangeReadingEntryStateError::Conflict { .. })))
        .count();
    assert_eq!(successes, 1, "exactly one command may commit");
    assert_eq!(conflicts, 1, "the losing command is visible, not retried");
    assert_eq!(
        progress
            .execute()
            .await
            .expect("progress after contention")
            .completed_entries,
        1,
        "the synchronous derived state changes exactly once"
    );

    let mut cleanup = database.begin().await.expect("begin final cleanup");
    sqlx::query("DELETE FROM reading_queue_entries")
        .execute(&mut *cleanup)
        .await
        .expect("remove contention entry");
    sqlx::query("UPDATE reading_progress SET completed_entries = 0 WHERE singleton")
        .execute(&mut *cleanup)
        .await
        .expect("reset progress after contention");
    cleanup.commit().await.expect("commit final cleanup");
}

#[tokio::test]
#[ignore = "requires an isolated migrated PostgreSQL database prepared explicitly for integration tests"]
async fn reading_queue_keyset_pages_preserve_order_filter_context_and_termination() {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL for a disposable test database");
    apply_migrations(&database_url)
        .await
        .expect("apply compiled migrations");
    let database = Database::connect(&database_url, 2)
        .await
        .expect("connect to PostgreSQL");
    let mut fixture = database.begin().await.expect("begin pagination fixture");
    sqlx::query("DELETE FROM reading_queue_entries")
        .execute(&mut *fixture)
        .await
        .expect("clear reading queue fixture");
    for (id, title, state, created_at) in [
        (
            "00000000-0000-0000-0000-000000000001",
            "Queued first tie",
            "queued",
            "2026-09-03T01:00:00Z",
        ),
        (
            "00000000-0000-0000-0000-000000000002",
            "Queued second tie",
            "queued",
            "2026-09-03T01:00:00Z",
        ),
        (
            "00000000-0000-0000-0000-000000000003",
            "Completed between pages",
            "completed",
            "2026-09-03T01:30:00Z",
        ),
        (
            "00000000-0000-0000-0000-000000000004",
            "Queued last",
            "queued",
            "2026-09-03T02:00:00Z",
        ),
    ] {
        sqlx::query(
            "INSERT INTO reading_queue_entries (id, title, source_url, state, created_at) VALUES ($1::uuid, $2, $3, $4, $5::timestamptz)",
        )
        .bind(id)
        .bind(title)
        .bind(format!("https://example.test/{id}"))
        .bind(state)
        .bind(created_at)
        .execute(&mut *fixture)
        .await
        .expect("insert deterministic pagination row");
    }
    fixture.commit().await.expect("commit pagination fixture");

    let list =
        ListReadingEntries::new(database.clone(), CURSOR_SIGNING_KEY).expect("valid cursor key");
    let first = list
        .execute(ListReadingEntriesQuery {
            status: Some("queued".to_owned()),
            sort: Some("oldest".to_owned()),
            limit: Some(2),
            cursor: None,
            authorization_scope: "anonymous".to_owned(),
        })
        .await
        .expect("first queued page");
    assert_eq!(
        first
            .entries
            .iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        ["Queued first tie", "Queued second tie"]
    );
    let first_cursor = first.next_cursor.expect("first page continues");
    let second = list
        .execute(ListReadingEntriesQuery {
            status: Some("queued".to_owned()),
            sort: Some("oldest".to_owned()),
            limit: Some(2),
            cursor: Some(first_cursor.clone()),
            authorization_scope: "anonymous".to_owned(),
        })
        .await
        .expect("second queued page");
    assert_eq!(second.entries[0].title, "Queued last");
    assert!(second.next_cursor.is_none(), "the final page terminates");

    let context_mismatch = list
        .execute(ListReadingEntriesQuery {
            status: Some("completed".to_owned()),
            sort: Some("oldest".to_owned()),
            limit: Some(2),
            cursor: Some(first_cursor),
            authorization_scope: "anonymous".to_owned(),
        })
        .await
        .expect_err("a cursor cannot cross filter context");
    assert!(matches!(
        context_mismatch,
        ListReadingEntriesError::InvalidCursor
    ));

    let newest = list
        .execute(ListReadingEntriesQuery {
            status: None,
            sort: Some("newest".to_owned()),
            limit: Some(2),
            cursor: None,
            authorization_scope: "anonymous".to_owned(),
        })
        .await
        .expect("newest page");
    assert_eq!(
        newest
            .entries
            .iter()
            .map(|entry| entry.title.as_str())
            .collect::<Vec<_>>(),
        ["Queued last", "Completed between pages"]
    );

    let mut cleanup = database.begin().await.expect("begin final cleanup");
    sqlx::query("DELETE FROM reading_queue_entries")
        .execute(&mut *cleanup)
        .await
        .expect("remove pagination fixture rows");
    cleanup.commit().await.expect("commit final cleanup");
}
