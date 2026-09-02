// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use std::env;

use product_application::{
    ChangeReadingEntryState, ChangeReadingEntryStateCommand, ChangeReadingEntryStateError,
    CreateReadingEntry, CreateReadingEntryCommand, CreateReadingEntryError, ListReadingEntries,
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
#[ignore = "requires an isolated migrated PostgreSQL database supplied by yydra check"]
async fn reading_queue_use_cases_commit_success_and_rollback_failures() {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL from yydra check");
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
#[ignore = "requires an isolated migrated PostgreSQL database supplied by yydra check"]
async fn reading_queue_keyset_pages_preserve_order_filter_context_and_termination() {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL from yydra check");
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
