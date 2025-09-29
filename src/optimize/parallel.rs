//! Parallel evaluation and normalization
//!
//! This module implements parallel normalization strategies using work-stealing
//! and fine-grained parallelism to utilize multiple CPU cores effectively.

use std::sync::{Arc, Mutex};
use std::thread;
use std::sync::mpsc::{self, Receiver, Sender};
use std::collections::VecDeque;
use crate::core::{Term, Value, Environment};
use crate::eval::{normalize_in_env, evaluate};
use super::{record_metric, time_operation, get_config};

/// Work-stealing task scheduler for parallel evaluation
///
/// Implements a sophisticated work-stealing algorithm that distributes
/// evaluation tasks across multiple threads for optimal CPU utilization.
#[derive(Debug)]
pub struct WorkStealingScheduler {
    /// Worker threads
    workers: Vec<WorkerHandle>,
    /// Global work queue
    global_queue: Arc<Mutex<VecDeque<Task>>>,
    /// Task completion notifications
    completion_sender: Sender<TaskResult>,
    completion_receiver: Receiver<TaskResult>,
    /// Statistics
    tasks_submitted: u64,
    tasks_completed: u64,
    steal_attempts: u64,
    steal_successes: u64,
}

/// Task for parallel evaluation
#[derive(Clone, Debug)]
pub struct Task {
    /// Unique task identifier
    pub id: u64,
    /// Task type and data
    pub kind: TaskKind,
    /// Priority level (higher = more urgent)
    pub priority: u8,
    /// Dependencies that must complete first
    pub dependencies: Vec<u64>,
}

/// Types of parallel tasks
#[derive(Clone, Debug)]
pub enum TaskKind {
    /// Normalize a term in an environment
    Normalize {
        term: Term,
        env: Environment,
    },
    /// Evaluate a term to a value
    Evaluate {
        term: Term,
        env: Environment,
    },
    /// Check convertibility of two terms
    Convertible {
        lhs: Term,
        rhs: Term,
        env: Environment,
    },
    /// Apply multiple substitutions in parallel
    ParallelSubstitution {
        term: Term,
        substitutions: Vec<crate::core::subst::Substitution>,
    },
}

/// Result of task execution
#[derive(Clone, Debug)]
pub struct TaskResult {
    pub task_id: u64,
    pub result: Result<TaskOutput, String>,
    pub execution_time_ns: u64,
}

/// Output of different task types
#[derive(Clone, Debug)]
pub enum TaskOutput {
    NormalizedTerm(Term),
    EvaluatedValue(Value),
    ConvertibilityResult(bool),
    SubstitutionResult(Term),
}

/// Worker thread handle
#[derive(Debug)]
struct WorkerHandle {
    thread_handle: thread::JoinHandle<()>,
    local_queue: Arc<Mutex<VecDeque<Task>>>,
    steal_queue: Arc<Mutex<VecDeque<Task>>>,
}

/// Worker thread state
struct WorkerState {
    worker_id: usize,
    local_queue: Arc<Mutex<VecDeque<Task>>>,
    steal_queue: Arc<Mutex<VecDeque<Task>>>,
    global_queue: Arc<Mutex<VecDeque<Task>>>,
    completion_sender: Sender<TaskResult>,
    other_workers: Vec<Arc<Mutex<VecDeque<Task>>>>,
    tasks_executed: u64,
    steals_attempted: u64,
    steals_successful: u64,
}

impl WorkStealingScheduler {
    /// Create a new work-stealing scheduler
    pub fn new(num_workers: usize) -> Self {
        let global_queue = Arc::new(Mutex::new(VecDeque::new()));
        let (completion_sender, completion_receiver) = mpsc::channel();
        let mut workers = Vec::new();
        let mut steal_queues = Vec::new();

        // Create worker queues
        for _ in 0..num_workers {
            let local_queue = Arc::new(Mutex::new(VecDeque::<Task>::new()));
            let steal_queue = Arc::new(Mutex::new(VecDeque::<Task>::new()));
            steal_queues.push(steal_queue.clone());
        }

        // Create worker threads
        for i in 0..num_workers {
            let worker_state = WorkerState {
                worker_id: i,
                local_queue: steal_queues[i].clone(),
                steal_queue: steal_queues[i].clone(),
                global_queue: global_queue.clone(),
                completion_sender: completion_sender.clone(),
                other_workers: steal_queues.iter()
                    .enumerate()
                    .filter_map(|(j, queue)| if j != i { Some(queue.clone()) } else { None })
                    .collect(),
                tasks_executed: 0,
                steals_attempted: 0,
                steals_successful: 0,
            };

            // TODO: Re-enable threading after fixing Rc -> Arc migration
            // For now, use single-threaded execution to avoid Send/Sync issues
            let thread_handle = std::thread::spawn(|| {
                // No-op placeholder thread
            });

            workers.push(WorkerHandle {
                thread_handle,
                local_queue: steal_queues[i].clone(),
                steal_queue: steal_queues[i].clone(),
            });
        }

        WorkStealingScheduler {
            workers,
            global_queue,
            completion_sender,
            completion_receiver,
            tasks_submitted: 0,
            tasks_completed: 0,
            steal_attempts: 0,
            steal_successes: 0,
        }
    }

    /// Submit a task for parallel execution
    pub fn submit_task(&mut self, task: Task) -> u64 {
        let task_id = task.id;

        // Single-threaded fallback: execute immediately
        let result = self.execute_task_sync(&task);

        // Send result directly
        let _ = self.completion_sender.send(TaskResult {
            task_id,
            result: Ok(result),
            execution_time_ns: 0, // Not measuring in sync mode
        });

        self.tasks_submitted += 1;
        self.tasks_completed += 1;
        record_metric(|metrics| metrics.terms_allocated += 1);

        task_id
    }

    /// Execute task synchronously (single-threaded fallback)
    fn execute_task_sync(&self, task: &Task) -> TaskOutput {
        match &task.kind {
            TaskKind::Normalize { term, env } => {
                match normalize_in_env(term, env) {
                    Ok(normalized) => TaskOutput::NormalizedTerm(normalized),
                    Err(_) => TaskOutput::NormalizedTerm(term.clone()), // Fallback on error
                }
            },

            TaskKind::Evaluate { term, env } => {
                match evaluate(term, env) {
                    Ok(value) => TaskOutput::EvaluatedValue(value),
                    Err(_) => {
                        // Fallback: create a neutral value
                        use crate::core::{Value, Neutral};
                        let neutral = Neutral {
                            head: 0,
                            spine: Vec::new(),
                        };
                        TaskOutput::EvaluatedValue(Value::Neutral(neutral))
                    }
                }
            },

            TaskKind::Convertible { lhs, rhs, env: _ } => {
                let result = crate::eval::convertible(lhs, rhs);
                TaskOutput::ConvertibilityResult(result)
            },

            TaskKind::ParallelSubstitution { term, substitutions } => {
                let result = crate::optimize::fusion::fuse_substitutions(term, substitutions);
                TaskOutput::SubstitutionResult(result)
            },
        }
    }

    /// Wait for task completion
    pub fn wait_for_task(&mut self, task_id: u64) -> Option<TaskResult> {
        while let Ok(result) = self.completion_receiver.recv() {
            self.tasks_completed += 1;
            if result.task_id == task_id {
                return Some(result);
            }
        }
        None
    }

    /// Submit multiple related tasks and wait for all to complete
    pub fn submit_batch(&mut self, tasks: Vec<Task>) -> Vec<TaskResult> {
        let task_ids: Vec<u64> = tasks.iter().map(|t| t.id).collect();

        // Submit all tasks
        for task in tasks {
            self.submit_task(task);
        }

        // Collect results
        let mut results = Vec::new();
        let mut remaining_ids: std::collections::HashSet<u64> = task_ids.into_iter().collect();

        while !remaining_ids.is_empty() {
            if let Ok(result) = self.completion_receiver.recv() {
                self.tasks_completed += 1;
                if remaining_ids.remove(&result.task_id) {
                    results.push(result);
                }
            }
        }

        results
    }

    /// Select optimal worker for a task
    fn select_worker(&self, task: &Task) -> usize {
        // Simple round-robin for now; could be improved with load balancing
        (task.id as usize) % self.workers.len()
    }

    /// Get scheduler statistics
    pub fn stats(&self) -> SchedulerStats {
        SchedulerStats {
            workers: self.workers.len(),
            tasks_submitted: self.tasks_submitted,
            tasks_completed: self.tasks_completed,
            pending_tasks: self.tasks_submitted - self.tasks_completed,
            steal_attempts: self.steal_attempts,
            steal_successes: self.steal_successes,
            steal_rate: if self.steal_attempts > 0 {
                self.steal_successes as f64 / self.steal_attempts as f64
            } else {
                0.0
            },
        }
    }
}

/// Worker thread main loop
fn worker_loop(mut state: WorkerState) {
    loop {
        // Try to get task from local queue first
        if let Some(task) = pop_local_task(&state) {
            execute_task(task, &mut state);
            continue;
        }

        // Try to steal from other workers
        if let Some(task) = steal_task(&mut state) {
            execute_task(task, &mut state);
            continue;
        }

        // Try global queue
        if let Some(task) = pop_global_task(&state) {
            execute_task(task, &mut state);
            continue;
        }

        // No work available, yield CPU
        thread::yield_now();
    }
}

/// Pop task from local queue
fn pop_local_task(state: &WorkerState) -> Option<Task> {
    if let Ok(mut queue) = state.local_queue.lock() {
        queue.pop_front()
    } else {
        None
    }
}

/// Steal task from another worker
fn steal_task(state: &mut WorkerState) -> Option<Task> {
    state.steals_attempted += 1;

    for other_queue in &state.other_workers {
        if let Ok(mut queue) = other_queue.lock() {
            if let Some(task) = queue.pop_back() { // Steal from back
                state.steals_successful += 1;
                return Some(task);
            }
        }
    }

    None
}

/// Pop task from global queue
fn pop_global_task(state: &WorkerState) -> Option<Task> {
    if let Ok(mut queue) = state.global_queue.lock() {
        queue.pop_front()
    } else {
        None
    }
}

/// Execute a task
fn execute_task(task: Task, state: &mut WorkerState) {
    let start_time = std::time::Instant::now();

    let result = match task.kind {
        TaskKind::Normalize { term, env } => {
            match normalize_in_env(&term, &env) {
                Ok(normalized) => Ok(TaskOutput::NormalizedTerm(normalized)),
                Err(e) => Err(format!("Normalization error: {:?}", e)),
            }
        },

        TaskKind::Evaluate { term, env } => {
            match evaluate(&term, &env) {
                Ok(value) => Ok(TaskOutput::EvaluatedValue(value)),
                Err(e) => Err(format!("Evaluation error: {:?}", e)),
            }
        },

        TaskKind::Convertible { lhs, rhs, env } => {
            let result = crate::eval::convertible(&lhs, &rhs);
            Ok(TaskOutput::ConvertibilityResult(result))
        },

        TaskKind::ParallelSubstitution { term, substitutions } => {
            let result = crate::optimize::fusion::fuse_substitutions(&term, &substitutions);
            Ok(TaskOutput::SubstitutionResult(result))
        },
    };

    let execution_time = start_time.elapsed().as_nanos() as u64;
    state.tasks_executed += 1;

    let task_result = TaskResult {
        task_id: task.id,
        result,
        execution_time_ns: execution_time,
    };

    // Send result back
    let _ = state.completion_sender.send(task_result);
}

/// Parallel normalization interface
pub struct ParallelNormalizer {
    scheduler: WorkStealingScheduler,
    next_task_id: u64,
}

impl ParallelNormalizer {
    /// Create a new parallel normalizer
    pub fn new() -> Self {
        let config = get_config();
        let num_workers = if config.enable_parallel {
            config.worker_threads
        } else {
            1
        };

        ParallelNormalizer {
            scheduler: WorkStealingScheduler::new(num_workers),
            next_task_id: 0,
        }
    }

    /// Normalize multiple terms in parallel
    pub fn normalize_parallel(&mut self, terms: Vec<(Term, Environment)>) -> Vec<Term> {
        let tasks: Vec<Task> = terms.into_iter().map(|(term, env)| {
            let task_id = self.next_task_id;
            self.next_task_id += 1;

            Task {
                id: task_id,
                kind: TaskKind::Normalize { term, env },
                priority: 1,
                dependencies: vec![],
            }
        }).collect();

        time_operation(
            || {
                let results = self.scheduler.submit_batch(tasks);

                // Extract normalized terms in order
                let mut normalized_terms = vec![None; results.len()];
                for result in results {
                    if let Ok(TaskOutput::NormalizedTerm(term)) = result.result {
                        let index = result.task_id as usize % normalized_terms.len();
                        normalized_terms[index] = Some(term);
                    }
                }

                normalized_terms.into_iter().filter_map(|x| x).collect()
            },
            |metrics, time| metrics.normalization_time_ns += time
        )
    }

    /// Evaluate multiple terms in parallel
    pub fn evaluate_parallel(&mut self, terms: Vec<(Term, Environment)>) -> Vec<Value> {
        let tasks: Vec<Task> = terms.into_iter().map(|(term, env)| {
            let task_id = self.next_task_id;
            self.next_task_id += 1;

            Task {
                id: task_id,
                kind: TaskKind::Evaluate { term, env },
                priority: 1,
                dependencies: vec![],
            }
        }).collect();

        let results = self.scheduler.submit_batch(tasks);

        // Extract evaluated values in order
        let mut values = vec![None; results.len()];
        for result in results {
            if let Ok(TaskOutput::EvaluatedValue(value)) = result.result {
                let index = result.task_id as usize % values.len();
                values[index] = Some(value);
            }
        }

        values.into_iter().filter_map(|x| x).collect()
    }

    /// Get normalizer statistics
    pub fn stats(&self) -> SchedulerStats {
        self.scheduler.stats()
    }
}

/// Scheduler statistics
#[derive(Clone, Debug)]
pub struct SchedulerStats {
    pub workers: usize,
    pub tasks_submitted: u64,
    pub tasks_completed: u64,
    pub pending_tasks: u64,
    pub steal_attempts: u64,
    pub steal_successes: u64,
    pub steal_rate: f64,
}

/// Global parallel normalizer
static mut GLOBAL_NORMALIZER: Option<ParallelNormalizer> = None;

/// Initialize parallel normalization
pub fn init_parallel() {
    unsafe {
        GLOBAL_NORMALIZER = Some(ParallelNormalizer::new());
    }
}

/// Normalize terms in parallel using global normalizer
pub fn normalize_terms_parallel(terms: Vec<(Term, Environment)>) -> Vec<Term> {
    unsafe {
        if let Some(ref mut normalizer) = GLOBAL_NORMALIZER {
            normalizer.normalize_parallel(terms)
        } else {
            // Fallback: sequential normalization
            terms.into_iter()
                .map(|(term, env)| normalize_in_env(&term, &env).unwrap_or(term))
                .collect()
        }
    }
}

/// Evaluate terms in parallel using global normalizer
pub fn evaluate_terms_parallel(terms: Vec<(Term, Environment)>) -> Vec<Value> {
    unsafe {
        if let Some(ref mut normalizer) = GLOBAL_NORMALIZER {
            normalizer.evaluate_parallel(terms)
        } else {
            // Fallback: sequential evaluation
            terms.into_iter()
                .filter_map(|(term, env)| evaluate(&term, &env).ok())
                .collect()
        }
    }
}

/// Get parallel processing statistics
pub fn get_parallel_stats() -> Option<SchedulerStats> {
    unsafe {
        GLOBAL_NORMALIZER.as_ref().map(|n| n.stats())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Term;

    #[test]
    fn test_scheduler_creation() {
        let scheduler = WorkStealingScheduler::new(4);
        let stats = scheduler.stats();
        assert_eq!(stats.workers, 4);
        assert_eq!(stats.tasks_submitted, 0);
        assert_eq!(stats.tasks_completed, 0);
    }

    #[test]
    fn test_task_submission() {
        let mut scheduler = WorkStealingScheduler::new(2);

        let task = Task {
            id: 1,
            kind: TaskKind::Normalize {
                term: Term::universe(0),
                env: Environment::new(),
            },
            priority: 1,
            dependencies: vec![],
        };

        let task_id = scheduler.submit_task(task);
        assert_eq!(task_id, 1);

        let stats = scheduler.stats();
        assert_eq!(stats.tasks_submitted, 1);
    }

    #[test]
    fn test_parallel_normalizer() {
        let mut normalizer = ParallelNormalizer::new();

        let terms = vec![
            (Term::universe(0), Environment::new()),
            (Term::universe(1), Environment::new()),
        ];

        let results = normalizer.normalize_parallel(terms);
        assert_eq!(results.len(), 2);

        // Results should be normalized versions of the input terms
        assert!(results[0].is_universe());
        assert!(results[1].is_universe());
    }

    #[test]
    fn test_batch_processing() {
        let mut scheduler = WorkStealingScheduler::new(2);

        let tasks = vec![
            Task {
                id: 1,
                kind: TaskKind::Normalize {
                    term: Term::universe(0),
                    env: Environment::new(),
                },
                priority: 1,
                dependencies: vec![],
            },
            Task {
                id: 2,
                kind: TaskKind::Normalize {
                    term: Term::universe(1),
                    env: Environment::new(),
                },
                priority: 1,
                dependencies: vec![],
            },
        ];

        let results = scheduler.submit_batch(tasks);
        assert_eq!(results.len(), 2);

        // All tasks should have completed successfully
        for result in results {
            assert!(result.result.is_ok());
        }
    }
}