// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bounded, explicitly lossy work that may run only after an authoritative commit.
//!
//! This is not a durable queue. Admission rejection, failure, timeout, cancellation,
//! panic, forced shutdown, or process crash can lose the work and never changes the
//! already committed command result. Business invariants must remain synchronous.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{Mutex, broadcast, mpsc, watch};
use tokio::task::{Id, JoinHandle, JoinSet};
use tokio::time::{Instant, sleep_until, timeout_at};
use tracing::Instrument;

const MAX_TASK_NAME_LENGTH: usize = 64;
const MAX_TASK_DEADLINE: Duration = Duration::from_secs(24 * 60 * 60);
const EVENT_CAPACITY: usize = 128;

type BoxTaskError = Box<dyn Error + Send + Sync>;
type TaskFuture = Pin<Box<dyn Future<Output = Result<(), BoxTaskError>> + Send + 'static>>;
type TaskFactory = Box<dyn FnOnce(PostCommitCancellation) -> TaskFuture + Send + 'static>;

pub struct LossyPostCommitTask {
    name: String,
    deadline_after_admission: Duration,
    admitted_deadline: Option<Instant>,
    work: TaskFactory,
}

impl LossyPostCommitTask {
    pub fn new<N, F, Fut, E>(
        name: N,
        deadline_after_admission: Duration,
        work: F,
    ) -> Result<Self, PostCommitTaskBuildError>
    where
        N: Into<String>,
        F: FnOnce(PostCommitCancellation) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        E: Error + Send + Sync + 'static,
    {
        let name = name.into();
        if name.is_empty()
            || name.len() > MAX_TASK_NAME_LENGTH
            || !name.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
            || !name
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            || deadline_after_admission.is_zero()
            || deadline_after_admission > MAX_TASK_DEADLINE
        {
            return Err(PostCommitTaskBuildError);
        }
        Ok(Self {
            name,
            deadline_after_admission,
            admitted_deadline: None,
            work: Box::new(move |cancellation| {
                let future = work(cancellation);
                Box::pin(async move {
                    future
                        .await
                        .map_err(|error| Box::new(error) as BoxTaskError)
                })
            }),
        })
    }
}

#[derive(Clone)]
pub struct PostCommitCancellation {
    shutdown: watch::Receiver<bool>,
}

impl PostCommitCancellation {
    pub fn is_cancelled(&self) -> bool {
        *self.shutdown.borrow()
    }

    pub async fn cancelled(&mut self) {
        if self.is_cancelled() {
            return;
        }
        while self.shutdown.changed().await.is_ok() {
            if self.is_cancelled() {
                return;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PostCommitExecutorConfig {
    pub queue_capacity: usize,
    pub max_concurrency: usize,
    pub shutdown_grace: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostCommitTaskOutcome {
    Admitted,
    AdmissionRejected,
    Started,
    Completed,
    Failed,
    TimedOut,
    Cancelled,
    Panicked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostCommitEvent {
    pub task_name: String,
    pub outcome: PostCommitTaskOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionFailure {
    Capacity,
    Closed,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PostCommitAdmissionError {
    pub task_name: String,
    pub reason: AdmissionFailure,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PostCommitMetricsSnapshot {
    pub admitted: u64,
    pub rejected: u64,
    pub completed: u64,
    pub failed: u64,
    pub timed_out: u64,
    pub cancelled: u64,
    pub panicked: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostCommitShutdownOutcome {
    Graceful,
    DeadlineExceeded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostCommitShutdownReport {
    pub outcome: PostCommitShutdownOutcome,
    pub metrics: PostCommitMetricsSnapshot,
}

#[derive(Clone)]
pub struct PostCommitExecutor {
    inner: Arc<ExecutorInner>,
}

struct ExecutorInner {
    sender: mpsc::Sender<LossyPostCommitTask>,
    shutdown: watch::Sender<bool>,
    events: broadcast::Sender<PostCommitEvent>,
    metrics: Arc<PostCommitMetrics>,
    accepting: AtomicBool,
    manager: Mutex<Option<JoinHandle<PostCommitShutdownReport>>>,
    report: Mutex<Option<PostCommitShutdownReport>>,
}

impl PostCommitExecutor {
    pub fn start(config: PostCommitExecutorConfig) -> Result<Self, PostCommitConfigurationError> {
        if config.queue_capacity == 0
            || config.max_concurrency == 0
            || config.shutdown_grace.is_zero()
            || config.shutdown_grace > MAX_TASK_DEADLINE
        {
            return Err(PostCommitConfigurationError);
        }
        tokio::runtime::Handle::try_current().map_err(|_| PostCommitConfigurationError)?;
        let (sender, receiver) = mpsc::channel(config.queue_capacity);
        let (shutdown, shutdown_receiver) = watch::channel(false);
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let metrics = Arc::new(PostCommitMetrics::default());
        let manager = tokio::spawn(run_manager(
            receiver,
            shutdown_receiver,
            events.clone(),
            metrics.clone(),
            config,
        ));
        Ok(Self {
            inner: Arc::new(ExecutorInner {
                sender,
                shutdown,
                events,
                metrics,
                accepting: AtomicBool::new(true),
                manager: Mutex::new(Some(manager)),
                report: Mutex::new(None),
            }),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<PostCommitEvent> {
        self.inner.events.subscribe()
    }

    pub fn metrics(&self) -> PostCommitMetricsSnapshot {
        self.inner.metrics.snapshot()
    }

    pub fn try_submit(
        &self,
        mut task: LossyPostCommitTask,
    ) -> Result<(), PostCommitAdmissionError> {
        if !self.inner.accepting.load(Ordering::SeqCst) {
            return Err(self.reject(task, AdmissionFailure::Closed));
        }
        task.admitted_deadline = Some(Instant::now() + task.deadline_after_admission);
        let name = task.name.clone();
        match self.inner.sender.try_send(task) {
            Ok(()) => {
                self.inner.metrics.admitted.fetch_add(1, Ordering::SeqCst);
                emit_event(
                    &self.inner.events,
                    PostCommitEvent {
                        task_name: name,
                        outcome: PostCommitTaskOutcome::Admitted,
                    },
                );
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(task)) => {
                Err(self.reject(task, AdmissionFailure::Capacity))
            }
            Err(mpsc::error::TrySendError::Closed(task)) => {
                Err(self.reject(task, AdmissionFailure::Closed))
            }
        }
    }

    fn reject(
        &self,
        task: LossyPostCommitTask,
        reason: AdmissionFailure,
    ) -> PostCommitAdmissionError {
        self.inner.metrics.rejected.fetch_add(1, Ordering::SeqCst);
        emit_event(
            &self.inner.events,
            PostCommitEvent {
                task_name: task.name.clone(),
                outcome: PostCommitTaskOutcome::AdmissionRejected,
            },
        );
        PostCommitAdmissionError {
            task_name: task.name,
            reason,
        }
    }

    pub async fn shutdown(&self) -> PostCommitShutdownReport {
        self.inner.accepting.store(false, Ordering::SeqCst);
        let _ = self.inner.shutdown.send(true);
        let mut manager = self.inner.manager.lock().await;
        if let Some(manager) = manager.take() {
            let report = manager.await.unwrap_or_else(|_| PostCommitShutdownReport {
                outcome: PostCommitShutdownOutcome::DeadlineExceeded,
                metrics: self.inner.metrics.snapshot(),
            });
            *self.inner.report.lock().await = Some(report.clone());
            report
        } else {
            self.inner
                .report
                .lock()
                .await
                .clone()
                .unwrap_or(PostCommitShutdownReport {
                    outcome: PostCommitShutdownOutcome::Graceful,
                    metrics: self.inner.metrics.snapshot(),
                })
        }
    }
}

async fn run_manager(
    mut receiver: mpsc::Receiver<LossyPostCommitTask>,
    mut shutdown: watch::Receiver<bool>,
    events: broadcast::Sender<PostCommitEvent>,
    metrics: Arc<PostCommitMetrics>,
    config: PostCommitExecutorConfig,
) -> PostCommitShutdownReport {
    let mut active = JoinSet::new();
    let mut active_names = HashMap::<Id, String>::new();
    let mut shutting_down = false;
    let mut shutdown_deadline = None;

    loop {
        if shutting_down && active.is_empty() {
            return PostCommitShutdownReport {
                outcome: PostCommitShutdownOutcome::Graceful,
                metrics: metrics.snapshot(),
            };
        }
        tokio::select! {
            biased;

            _ = async {
                if let Some(deadline) = shutdown_deadline {
                    sleep_until(deadline).await;
                } else {
                    std::future::pending::<()>().await;
                }
            }, if shutting_down => {
                for name in active_names.values() {
                    terminal_event(
                        &events,
                        &metrics,
                        name.clone(),
                        PostCommitTaskOutcome::Cancelled,
                    );
                }
                active_names.clear();
                active.abort_all();
                while active.join_next().await.is_some() {}
                return PostCommitShutdownReport {
                    outcome: PostCommitShutdownOutcome::DeadlineExceeded,
                    metrics: metrics.snapshot(),
                };
            }
            changed = shutdown.changed(), if !shutting_down => {
                let _ = changed;
                begin_shutdown(
                    &mut receiver,
                    &events,
                    &metrics,
                    config.shutdown_grace,
                    &mut shutting_down,
                    &mut shutdown_deadline,
                );
            }
            joined = active.join_next_with_id(), if !active.is_empty() => {
                if let Some(joined) = joined {
                    match joined {
                        Ok((id, outcome)) => {
                            if let Some(name) = active_names.remove(&id) {
                                terminal_event(&events, &metrics, name, outcome);
                            }
                        }
                        Err(error) => {
                            if let Some(name) = active_names.remove(&error.id()) {
                                let outcome = if error.is_panic() {
                                    PostCommitTaskOutcome::Panicked
                                } else {
                                    PostCommitTaskOutcome::Cancelled
                                };
                                terminal_event(&events, &metrics, name, outcome);
                            }
                        }
                    }
                }
            }
            task = receiver.recv(), if !shutting_down && active.len() < config.max_concurrency => {
                match task {
                    Some(task) => {
                        let name = task.name.clone();
                        let task_deadline = task
                            .admitted_deadline
                            .expect("only admitted tasks enter the executor queue");
                        if task_deadline <= Instant::now() {
                            terminal_event(
                                &events,
                                &metrics,
                                name,
                                PostCommitTaskOutcome::TimedOut,
                            );
                            continue;
                        }
                        emit_event(
                            &events,
                            PostCommitEvent {
                                task_name: name.clone(),
                                outcome: PostCommitTaskOutcome::Started,
                            },
                        );
                        let cancellation = PostCommitCancellation {
                            shutdown: shutdown.clone(),
                        };
                        let cancellation_observer = cancellation.clone();
                        let work = task.work;
                        let task_span = tracing::info_span!(
                            target: "yydra::post_commit",
                            "post_commit_lossy_task",
                            task_name = %name,
                            lossy = true,
                            retry = false,
                        );
                        let handle = active.spawn(async move {
                            let result = timeout_at(task_deadline, work(cancellation)).await;
                            if cancellation_observer.is_cancelled() {
                                PostCommitTaskOutcome::Cancelled
                            } else {
                                match result {
                                    Ok(Ok(())) => PostCommitTaskOutcome::Completed,
                                    Ok(Err(_)) => PostCommitTaskOutcome::Failed,
                                    Err(_) => PostCommitTaskOutcome::TimedOut,
                                }
                            }
                        }.instrument(task_span));
                        active_names.insert(handle.id(), name);
                    }
                    None => {
                        begin_shutdown(
                            &mut receiver,
                            &events,
                            &metrics,
                            config.shutdown_grace,
                            &mut shutting_down,
                            &mut shutdown_deadline,
                        );
                    }
                }
            }
        }
    }
}

fn begin_shutdown(
    receiver: &mut mpsc::Receiver<LossyPostCommitTask>,
    events: &broadcast::Sender<PostCommitEvent>,
    metrics: &PostCommitMetrics,
    grace: Duration,
    shutting_down: &mut bool,
    shutdown_deadline: &mut Option<Instant>,
) {
    *shutting_down = true;
    *shutdown_deadline = Some(Instant::now() + grace);
    receiver.close();
    while let Ok(task) = receiver.try_recv() {
        terminal_event(events, metrics, task.name, PostCommitTaskOutcome::Cancelled);
    }
}

fn terminal_event(
    events: &broadcast::Sender<PostCommitEvent>,
    metrics: &PostCommitMetrics,
    task_name: String,
    outcome: PostCommitTaskOutcome,
) {
    match outcome {
        PostCommitTaskOutcome::Completed => &metrics.completed,
        PostCommitTaskOutcome::Failed => &metrics.failed,
        PostCommitTaskOutcome::TimedOut => &metrics.timed_out,
        PostCommitTaskOutcome::Cancelled => &metrics.cancelled,
        PostCommitTaskOutcome::Panicked => &metrics.panicked,
        PostCommitTaskOutcome::Admitted
        | PostCommitTaskOutcome::AdmissionRejected
        | PostCommitTaskOutcome::Started => {
            emit_event(events, PostCommitEvent { task_name, outcome });
            return;
        }
    }
    .fetch_add(1, Ordering::SeqCst);
    emit_event(events, PostCommitEvent { task_name, outcome });
}

fn emit_event(events: &broadcast::Sender<PostCommitEvent>, event: PostCommitEvent) {
    match event.outcome {
        PostCommitTaskOutcome::Failed
        | PostCommitTaskOutcome::TimedOut
        | PostCommitTaskOutcome::Cancelled
        | PostCommitTaskOutcome::Panicked
        | PostCommitTaskOutcome::AdmissionRejected => tracing::warn!(
            target: "yydra::post_commit",
            task_name = %event.task_name,
            outcome = ?event.outcome,
            lossy = true,
            retry = false,
            "post-commit task lifecycle"
        ),
        _ => tracing::info!(
            target: "yydra::post_commit",
            task_name = %event.task_name,
            outcome = ?event.outcome,
            lossy = true,
            retry = false,
            "post-commit task lifecycle"
        ),
    }
    let _ = events.send(event);
}

#[derive(Default)]
struct PostCommitMetrics {
    admitted: AtomicU64,
    rejected: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    timed_out: AtomicU64,
    cancelled: AtomicU64,
    panicked: AtomicU64,
}

impl PostCommitMetrics {
    fn snapshot(&self) -> PostCommitMetricsSnapshot {
        PostCommitMetricsSnapshot {
            admitted: self.admitted.load(Ordering::SeqCst),
            rejected: self.rejected.load(Ordering::SeqCst),
            completed: self.completed.load(Ordering::SeqCst),
            failed: self.failed.load(Ordering::SeqCst),
            timed_out: self.timed_out.load(Ordering::SeqCst),
            cancelled: self.cancelled.load(Ordering::SeqCst),
            panicked: self.panicked.load(Ordering::SeqCst),
        }
    }
}

#[derive(Debug)]
pub struct PostCommitTaskBuildError;

impl fmt::Display for PostCommitTaskBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "post-commit task name or deadline is invalid; use a stable lowercase name and a bounded non-zero deadline",
        )
    }
}

impl Error for PostCommitTaskBuildError {}

#[derive(Debug)]
pub struct PostCommitConfigurationError;

impl fmt::Display for PostCommitConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "post-commit executor requires bounded positive capacity, concurrency, shutdown grace, and an active Tokio runtime",
        )
    }
}

impl Error for PostCommitConfigurationError {}
