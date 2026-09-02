// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use std::env;

use product_application::{
    ChangeReadingEntryState, ChangeReadingEntryStateCommand, ChangeReadingEntryStateError,
    CreateReadingEntry, CreateReadingEntryCommand, CreateReadingEntryError, ListReadingEntries,
    ReadingQueueEntryState,
};
use product_persistence_postgres::{Database, apply_migrations};

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
    let list = ListReadingEntries::new(database.clone());
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
        list.execute().await.expect("list committed entry"),
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
        list.execute()
            .await
            .expect("conflict leaves committed state")[0]
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
        list.execute()
            .await
            .expect("list after Domain rejection")
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
        list.execute()
            .await
            .expect("list after transaction rollback")
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
