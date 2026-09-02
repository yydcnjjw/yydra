// SPDX-License-Identifier: MIT OR Apache-2.0

#![forbid(unsafe_code)]

use std::io;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use product_application::post_commit::{
    AdmissionFailure, LossyPostCommitTask, PostCommitExecutor, PostCommitExecutorConfig,
    PostCommitShutdownOutcome, PostCommitTaskOutcome,
};
use tokio::sync::{Notify, broadcast};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;

#[derive(Clone)]
struct TraceCapture {
    entries: Arc<Mutex<Vec<String>>>,
}

impl<S> tracing_subscriber::Layer<S> for TraceCapture
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        struct FieldVisitor(String);

        impl Visit for FieldVisitor {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                self.0.push_str(&format!(" {}={value:?}", field.name()));
            }
        }

        let mut visitor = FieldVisitor(event.metadata().target().to_owned());
        event.record(&mut visitor);
        self.entries
            .lock()
            .expect("trace capture lock")
            .push(visitor.0);
    }
}

async fn wait_for_outcome(
    events: &mut broadcast::Receiver<product_application::post_commit::PostCommitEvent>,
    name: &str,
    outcome: PostCommitTaskOutcome,
) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let event = events.recv().await.expect("post-commit event");
            if event.task_name == name && event.outcome == outcome {
                return;
            }
        }
    })
    .await
    .expect("expected post-commit outcome before timeout");
}

#[tokio::test(flavor = "current_thread")]
async fn bounded_lossy_executor_reports_admission_deadline_timeout_failure_and_crash_without_retry()
{
    let trace_entries = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::registry().with(TraceCapture {
        entries: trace_entries.clone(),
    });
    let _trace_guard = tracing::subscriber::set_default(subscriber);
    let executor = PostCommitExecutor::start(PostCommitExecutorConfig {
        queue_capacity: 1,
        max_concurrency: 1,
        shutdown_grace: Duration::from_millis(200),
    })
    .expect("valid executor configuration inside Tokio");
    let mut events = executor.subscribe();
    let release = Arc::new(Notify::new());
    let first_release = release.clone();
    executor
        .try_submit(
            LossyPostCommitTask::new(
                "reading-index.refresh",
                Duration::from_secs(2),
                move |_| async move {
                    first_release.notified().await;
                    Ok::<(), io::Error>(())
                },
            )
            .expect("valid named lossy task"),
        )
        .expect("admit active task");
    wait_for_outcome(
        &mut events,
        "reading-index.refresh",
        PostCommitTaskOutcome::Started,
    )
    .await;

    executor
        .try_submit(
            LossyPostCommitTask::new(
                "reading-metrics.record",
                Duration::from_secs(2),
                |_| async { Ok::<(), io::Error>(()) },
            )
            .expect("valid queued task"),
        )
        .expect("admit one queued task");
    let rejected = executor
        .try_submit(
            LossyPostCommitTask::new(
                "reading-preview.prefetch",
                Duration::from_secs(2),
                |_| async { Ok::<(), io::Error>(()) },
            )
            .expect("valid rejected task"),
        )
        .expect_err("bounded queue must reject excess admission");
    assert_eq!(rejected.reason, AdmissionFailure::Capacity);
    assert_eq!(rejected.task_name, "reading-preview.prefetch");
    release.notify_one();
    wait_for_outcome(
        &mut events,
        "reading-index.refresh",
        PostCommitTaskOutcome::Completed,
    )
    .await;
    wait_for_outcome(
        &mut events,
        "reading-metrics.record",
        PostCommitTaskOutcome::Completed,
    )
    .await;

    let deadline_release = Arc::new(Notify::new());
    let blocker_release = deadline_release.clone();
    executor
        .try_submit(
            LossyPostCommitTask::new(
                "reading-index.deadline-blocker",
                Duration::from_secs(2),
                move |_| async move {
                    blocker_release.notified().await;
                    Ok::<(), io::Error>(())
                },
            )
            .expect("valid deadline blocker"),
        )
        .expect("admit deadline blocker");
    wait_for_outcome(
        &mut events,
        "reading-index.deadline-blocker",
        PostCommitTaskOutcome::Started,
    )
    .await;
    let queued_deadline_runs = Arc::new(AtomicUsize::new(0));
    let observed_queued_deadline_runs = queued_deadline_runs.clone();
    executor
        .try_submit(
            LossyPostCommitTask::new(
                "reading-preview.queue-deadline",
                Duration::from_millis(20),
                move |_| async move {
                    observed_queued_deadline_runs.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), io::Error>(())
                },
            )
            .expect("valid queued deadline task"),
        )
        .expect("admit queued deadline task");
    tokio::time::sleep(Duration::from_millis(40)).await;
    deadline_release.notify_one();
    wait_for_outcome(
        &mut events,
        "reading-index.deadline-blocker",
        PostCommitTaskOutcome::Completed,
    )
    .await;
    wait_for_outcome(
        &mut events,
        "reading-preview.queue-deadline",
        PostCommitTaskOutcome::TimedOut,
    )
    .await;
    assert_eq!(
        queued_deadline_runs.load(Ordering::SeqCst),
        0,
        "a task that expires in the queue must never execute"
    );

    let failure_runs = Arc::new(AtomicUsize::new(0));
    let observed_runs = failure_runs.clone();
    executor
        .try_submit(
            LossyPostCommitTask::new(
                "reading-preview.fail",
                Duration::from_secs(1),
                move |_| async move {
                    observed_runs.fetch_add(1, Ordering::SeqCst);
                    Err(io::Error::other("fixture failure"))
                },
            )
            .expect("valid failing task"),
        )
        .expect("admit failing task");
    wait_for_outcome(
        &mut events,
        "reading-preview.fail",
        PostCommitTaskOutcome::Failed,
    )
    .await;
    assert_eq!(failure_runs.load(Ordering::SeqCst), 1, "no retry");

    executor
        .try_submit(
            LossyPostCommitTask::new(
                "reading-preview.timeout",
                Duration::from_millis(20),
                |_| async {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    Ok::<(), io::Error>(())
                },
            )
            .expect("valid deadline task"),
        )
        .expect("admit deadline task");
    wait_for_outcome(
        &mut events,
        "reading-preview.timeout",
        PostCommitTaskOutcome::TimedOut,
    )
    .await;

    executor
        .try_submit(
            LossyPostCommitTask::new("reading-preview.crash", Duration::from_secs(1), |_| async {
                panic!("task crash fixture");
                #[allow(unreachable_code)]
                Ok::<(), io::Error>(())
            })
            .expect("valid crashing task"),
        )
        .expect("admit crashing task");
    wait_for_outcome(
        &mut events,
        "reading-preview.crash",
        PostCommitTaskOutcome::Panicked,
    )
    .await;

    assert!(
        LossyPostCommitTask::new("dynamic secret task", Duration::from_secs(1), |_| async {
            Ok::<(), io::Error>(())
        })
        .is_err(),
        "task names are stable bounded identifiers, not arbitrary data"
    );
    let report = executor.shutdown().await;
    assert_eq!(report.outcome, PostCommitShutdownOutcome::Graceful);
    let metrics = report.metrics;
    assert_eq!(metrics.admitted, 7);
    assert_eq!(metrics.rejected, 1);
    assert_eq!(metrics.completed, 3);
    assert_eq!(metrics.failed, 1);
    assert_eq!(metrics.timed_out, 2);
    assert_eq!(metrics.panicked, 1);
    let trace_entries = trace_entries.lock().expect("trace entries").clone();
    assert!(
        trace_entries.iter().any(|entry| {
            entry.starts_with("yydra::post_commit")
                && entry.contains("lossy=true")
                && entry.contains("retry=false")
                && entry.contains("task_name=")
        }),
        "structured tracing exposes the task-name field and explicit lossy/no-retry policy; observed {trace_entries:?}"
    );
}

#[tokio::test]
async fn shutdown_cancels_tracked_work_and_forces_uncooperative_work_by_its_deadline() {
    let executor = PostCommitExecutor::start(PostCommitExecutorConfig {
        queue_capacity: 1,
        max_concurrency: 1,
        shutdown_grace: Duration::from_millis(200),
    })
    .expect("valid executor");
    let mut events = executor.subscribe();
    let observed_cancellation = Arc::new(AtomicBool::new(false));
    let task_observation = observed_cancellation.clone();
    executor
        .try_submit(
            LossyPostCommitTask::new(
                "reading-index.cancel",
                Duration::from_secs(5),
                move |mut cancellation| async move {
                    cancellation.cancelled().await;
                    task_observation.store(true, Ordering::SeqCst);
                    Ok::<(), io::Error>(())
                },
            )
            .expect("valid cancellable task"),
        )
        .expect("admit cancellable task");
    wait_for_outcome(
        &mut events,
        "reading-index.cancel",
        PostCommitTaskOutcome::Started,
    )
    .await;
    let queued_runs = Arc::new(AtomicUsize::new(0));
    let observed_queued_runs = queued_runs.clone();
    executor
        .try_submit(
            LossyPostCommitTask::new(
                "reading-index.queued",
                Duration::from_secs(5),
                move |_| async move {
                    observed_queued_runs.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), io::Error>(())
                },
            )
            .expect("valid queued task"),
        )
        .expect("admit queued task");
    let started = Instant::now();
    let report = executor.shutdown().await;
    assert_eq!(report.outcome, PostCommitShutdownOutcome::Graceful);
    assert!(started.elapsed() < Duration::from_millis(500));
    assert!(observed_cancellation.load(Ordering::SeqCst));
    assert_eq!(queued_runs.load(Ordering::SeqCst), 0);
    assert_eq!(report.metrics.cancelled, 2);

    let forced = PostCommitExecutor::start(PostCommitExecutorConfig {
        queue_capacity: 1,
        max_concurrency: 1,
        shutdown_grace: Duration::from_millis(30),
    })
    .expect("valid forced-shutdown executor");
    let mut forced_events = forced.subscribe();
    forced
        .try_submit(
            LossyPostCommitTask::new(
                "reading-index.uncooperative",
                Duration::from_secs(5),
                |_| async {
                    std::future::pending::<()>().await;
                    Ok::<(), io::Error>(())
                },
            )
            .expect("valid uncooperative task"),
        )
        .expect("admit uncooperative task");
    wait_for_outcome(
        &mut forced_events,
        "reading-index.uncooperative",
        PostCommitTaskOutcome::Started,
    )
    .await;
    let forced_started = Instant::now();
    let forced_report = forced.shutdown().await;
    assert_eq!(
        forced_report.outcome,
        PostCommitShutdownOutcome::DeadlineExceeded
    );
    assert!(forced_started.elapsed() < Duration::from_millis(500));
    assert_eq!(forced_report.metrics.cancelled, 1);
}
