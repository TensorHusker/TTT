//! Parallel TTT-Lean Translation Engine
//!
//! This module implements a high-performance parallel translation system
//! with work-stealing scheduler and dependency-aware task execution.

use std::sync::{Arc, atomic::{AtomicU64, AtomicUsize, Ordering}};
use std::collections::{HashMap, VecDeque};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock, Condvar};
use crossbeam::channel::{self, Receiver, Sender, TryRecvError};
use crossbeam::deque::{Injector, Stealer, Worker};
use rayon::{ThreadPool, ThreadPoolBuilder};
use dashmap::DashMap;

use crate::core::{Term, Level};
use super::{LeanTerm, LeanLevel, LeanName, Result, LeanError};
use super::optimize::OptimizedTranslator;
use super::translation::TranslationContext;

/// Unique identifier for translation tasks
pub type TaskId = u64;

/// Priority levels for translation tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Translation task with dependencies and metadata
#[derive(Debug, Clone)]
pub struct TranslationTask {
    pub id: TaskId,
    pub term: Term,
    pub priority: TaskPriority,
    pub dependencies: Vec<TaskId>,
    pub created_at: Instant,
    pub estimated_cost: u32,
}

impl TranslationTask {
    pub fn new(id: TaskId, term: Term) -> Self {
        Self {
            id,
            term,
            priority: TaskPriority::Normal,
            dependencies: Vec::new(),
            created_at: Instant::now(),
            estimated_cost: estimate_translation_cost(&term),
        }
    }

    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_dependencies(mut self, deps: Vec<TaskId>) -> Self {
        self.dependencies = deps;
        self
    }
}

/// Result of a completed translation task
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: TaskId,
    pub result: Result<LeanTerm>,
    pub execution_time: Duration,
    pub worker_id: usize,
}

/// Work-stealing scheduler for parallel task execution
pub struct WorkStealingScheduler {
    /// Global work queue for new tasks
    global_queue: Arc<Injector<TranslationTask>>,

    /// Per-worker local queues
    local_queues: Vec<Worker<TranslationTask>>,

    /// Stealers for work stealing
    stealers: Vec<Stealer<TranslationTask>>,

    /// Number of worker threads
    num_workers: usize,

    /// Task completion notification
    completion_tx: Sender<TaskResult>,
    completion_rx: Arc<Mutex<Receiver<TaskResult>>>,

    /// Task dependency tracking
    dependency_graph: Arc<RwLock<HashMap<TaskId, Vec<TaskId>>>>,
    pending_tasks: Arc<DashMap<TaskId, TranslationTask>>,
    ready_tasks: Arc<DashMap<TaskId, TranslationTask>>,

    /// Statistics
    tasks_submitted: AtomicU64,
    tasks_completed: AtomicU64,
    total_steal_attempts: AtomicU64,
    successful_steals: AtomicU64,
}

impl WorkStealingScheduler {
    pub fn new(num_workers: Option<usize>) -> Self {
        let num_workers = num_workers.unwrap_or_else(num_cpus::get);
        let (completion_tx, completion_rx) = channel::unbounded();

        let global_queue = Arc::new(Injector::new());
        let mut local_queues = Vec::with_capacity(num_workers);
        let mut stealers = Vec::with_capacity(num_workers);

        for _ in 0..num_workers {
            let worker = Worker::new_fifo();
            stealers.push(worker.stealer());
            local_queues.push(worker);
        }

        Self {
            global_queue,
            local_queues,
            stealers,
            num_workers,
            completion_tx,
            completion_rx: Arc::new(Mutex::new(completion_rx)),
            dependency_graph: Arc::new(RwLock::new(HashMap::new())),
            pending_tasks: Arc::new(DashMap::new()),
            ready_tasks: Arc::new(DashMap::new()),
            tasks_submitted: AtomicU64::new(0),
            tasks_completed: AtomicU64::new(0),
            total_steal_attempts: AtomicU64::new(0),
            successful_steals: AtomicU64::new(0),
        }
    }

    /// Submit a task for parallel execution
    pub fn submit_task(&self, task: TranslationTask) {
        self.tasks_submitted.fetch_add(1, Ordering::Relaxed);

        if task.dependencies.is_empty() {
            // Task is ready to execute
            self.ready_tasks.insert(task.id, task.clone());
            self.global_queue.push(task);
        } else {
            // Task has dependencies, add to pending
            {
                let mut graph = self.dependency_graph.write();
                for dep_id in &task.dependencies {
                    graph.entry(*dep_id).or_insert_with(Vec::new).push(task.id);
                }
            }
            self.pending_tasks.insert(task.id, task);
        }
    }

    /// Try to steal work from other workers
    fn try_steal_work(&self, worker_id: usize) -> Option<TranslationTask> {
        self.total_steal_attempts.fetch_add(1, Ordering::Relaxed);

        // Try stealing from other workers
        for (i, stealer) in self.stealers.iter().enumerate() {
            if i == worker_id { continue; }

            match stealer.steal() {
                crossbeam::deque::Steal::Success(task) => {
                    self.successful_steals.fetch_add(1, Ordering::Relaxed);
                    return Some(task);
                },
                _ => continue,
            }
        }

        // Try stealing from global queue
        self.global_queue.steal().success()
    }

    /// Get next task for a worker
    fn get_next_task(&self, worker_id: usize) -> Option<TranslationTask> {
        // First try local queue
        if let Some(task) = self.local_queues[worker_id].pop() {
            return Some(task);
        }

        // Then try work stealing
        self.try_steal_work(worker_id)
    }

    /// Handle task completion and dependency resolution
    fn handle_task_completion(&self, result: TaskResult) {
        self.tasks_completed.fetch_add(1, Ordering::Relaxed);

        // Remove from ready tasks
        self.ready_tasks.remove(&result.task_id);

        // Check for dependent tasks that are now ready
        let dependent_tasks = {
            let graph = self.dependency_graph.read();
            graph.get(&result.task_id).cloned().unwrap_or_default()
        };

        for dep_task_id in dependent_tasks {
            if let Some((_, mut task)) = self.pending_tasks.remove(&dep_task_id) {
                task.dependencies.retain(|&id| id != result.task_id);

                if task.dependencies.is_empty() {
                    // Task is now ready
                    self.ready_tasks.insert(task.id, task.clone());
                    self.global_queue.push(task);
                } else {
                    // Still has dependencies
                    self.pending_tasks.insert(dep_task_id, task);
                }
            }
        }
    }

    /// Get steal efficiency ratio
    pub fn steal_efficiency(&self) -> f64 {
        let attempts = self.total_steal_attempts.load(Ordering::Relaxed);
        let successes = self.successful_steals.load(Ordering::Relaxed);
        if attempts > 0 { successes as f64 / attempts as f64 } else { 0.0 }
    }
}

/// High-performance parallel translator
pub struct ParallelTranslator {
    /// Work-stealing scheduler
    scheduler: Arc<WorkStealingScheduler>,

    /// Optimized translator for individual tasks
    translator: Arc<OptimizedTranslator>,

    /// Thread pool for worker threads
    thread_pool: ThreadPool,

    /// Completed results
    completed_results: Arc<DashMap<TaskId, TaskResult>>,

    /// Task ID counter
    next_task_id: AtomicU64,

    /// Performance metrics
    pub metrics: ParallelMetrics,
}

/// Performance metrics for parallel translation
#[derive(Debug)]
pub struct ParallelMetrics {
    pub tasks_per_second: AtomicU64,
    pub average_task_time: AtomicU64,
    pub worker_utilization: Vec<AtomicU64>,
    pub queue_depth: AtomicUsize,
    pub parallel_efficiency: AtomicU64,
}

impl ParallelMetrics {
    pub fn new(num_workers: usize) -> Self {
        let mut worker_utilization = Vec::with_capacity(num_workers);
        for _ in 0..num_workers {
            worker_utilization.push(AtomicU64::new(0));
        }

        Self {
            tasks_per_second: AtomicU64::new(0),
            average_task_time: AtomicU64::new(0),
            worker_utilization,
            queue_depth: AtomicUsize::new(0),
            parallel_efficiency: AtomicU64::new(0),
        }
    }

    pub fn worker_utilization_rate(&self, worker_id: usize) -> f64 {
        if worker_id < self.worker_utilization.len() {
            let utilization = self.worker_utilization[worker_id].load(Ordering::Relaxed);
            utilization as f64 / 100.0 // Assuming stored as percentage * 100
        } else {
            0.0
        }
    }

    pub fn overall_utilization(&self) -> f64 {
        let total: u64 = self.worker_utilization.iter()
            .map(|u| u.load(Ordering::Relaxed))
            .sum();
        let avg = total as f64 / self.worker_utilization.len() as f64;
        avg / 100.0
    }
}

impl ParallelTranslator {
    /// Create a new parallel translator
    pub fn new(num_workers: Option<usize>) -> Result<Self> {
        let num_workers = num_workers.unwrap_or_else(num_cpus::get);
        let scheduler = Arc::new(WorkStealingScheduler::new(Some(num_workers)));
        let translator = Arc::new(OptimizedTranslator::new());

        let thread_pool = ThreadPoolBuilder::new()
            .num_threads(num_workers)
            .thread_name(|i| format!("lean-translator-{}", i))
            .build()
            .map_err(|e| LeanError::TranslationError(format!("Failed to create thread pool: {}", e)))?;

        Ok(Self {
            scheduler,
            translator,
            thread_pool,
            completed_results: Arc::new(DashMap::new()),
            next_task_id: AtomicU64::new(1),
            metrics: ParallelMetrics::new(num_workers),
        })
    }

    /// Translate a single term with parallel execution
    pub fn translate_parallel(&self, term: &Term) -> Result<LeanTerm> {
        let task_id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
        let task = TranslationTask::new(task_id, term.clone());

        // For single terms, use direct translation unless it's complex
        if estimate_translation_cost(term) < 100 {
            return self.translator.translate_optimized(term);
        }

        // Submit complex term for parallel processing
        self.submit_task_and_wait(task)
    }

    /// Translate multiple terms in parallel
    pub fn translate_batch_parallel(&self, terms: &[Term]) -> Result<Vec<LeanTerm>> {
        let start_time = Instant::now();
        let mut tasks = Vec::with_capacity(terms.len());

        // Create tasks for each term
        for term in terms {
            let task_id = self.next_task_id.fetch_add(1, Ordering::Relaxed);
            let task = TranslationTask::new(task_id, term.clone());
            tasks.push(task);
        }

        // Submit all tasks
        for task in &tasks {
            self.scheduler.submit_task(task.clone());
        }

        // Collect results
        let mut results = Vec::with_capacity(tasks.len());
        for task in &tasks {
            match self.wait_for_task(task.id) {
                Ok(result) => results.push(result),
                Err(e) => return Err(e),
            }
        }

        let elapsed = start_time.elapsed();
        self.update_throughput_metrics(tasks.len(), elapsed);

        Ok(results)
    }

    /// Submit a task and wait for completion
    fn submit_task_and_wait(&self, task: TranslationTask) -> Result<LeanTerm> {
        let task_id = task.id;
        self.scheduler.submit_task(task);
        self.wait_for_task(task_id)
    }

    /// Wait for a specific task to complete
    fn wait_for_task(&self, task_id: TaskId) -> Result<LeanTerm> {
        // Check if already completed
        if let Some(result) = self.completed_results.get(&task_id) {
            return result.result.clone();
        }

        // Execute task synchronously if small or no workers available
        if let Some((_, task)) = self.scheduler.ready_tasks.remove(&task_id) {
            return self.translator.translate_optimized(&task.term);
        }

        // Wait for completion from worker thread
        let completion_rx = self.scheduler.completion_rx.lock();
        loop {
            match completion_rx.try_recv() {
                Ok(result) if result.task_id == task_id => {
                    let lean_term = result.result.clone();
                    self.completed_results.insert(task_id, result);
                    return lean_term;
                },
                Ok(result) => {
                    // Store other completed tasks
                    self.completed_results.insert(result.task_id, result);
                },
                Err(TryRecvError::Empty) => {
                    thread::sleep(Duration::from_micros(100));
                    continue;
                },
                Err(TryRecvError::Disconnected) => {
                    return Err(LeanError::TranslationError("Worker threads disconnected".to_string()));
                }
            }
        }
    }

    /// Start worker threads
    pub fn start_workers(&self) {
        let num_workers = self.scheduler.num_workers;

        for worker_id in 0..num_workers {
            let scheduler = Arc::clone(&self.scheduler);
            let translator = Arc::clone(&self.translator);
            let metrics = &self.metrics.worker_utilization[worker_id];
            let utilization_counter = metrics.clone();

            self.thread_pool.spawn(move || {
                let mut work_time = Duration::default();
                let mut idle_time = Duration::default();

                loop {
                    let task_start = Instant::now();

                    if let Some(task) = scheduler.get_next_task(worker_id) {
                        let execution_start = Instant::now();

                        // Execute translation
                        let result = translator.translate_optimized(&task.term);
                        let execution_time = execution_start.elapsed();

                        // Record result
                        let task_result = TaskResult {
                            task_id: task.id,
                            result,
                            execution_time,
                            worker_id,
                        };

                        // Handle completion
                        scheduler.handle_task_completion(task_result.clone());

                        // Send completion notification
                        if let Err(_) = scheduler.completion_tx.send(task_result) {
                            break; // Channel closed, shutdown
                        }

                        work_time += execution_time;
                    } else {
                        // No work available, brief sleep
                        thread::sleep(Duration::from_micros(10));
                        idle_time += Duration::from_micros(10);
                    }

                    // Update utilization metrics periodically
                    let total_time = work_time + idle_time;
                    if total_time.as_millis() > 1000 { // Every second
                        let utilization = (work_time.as_nanos() as f64 / total_time.as_nanos() as f64 * 10000.0) as u64;
                        utilization_counter.store(utilization, Ordering::Relaxed);
                        work_time = Duration::default();
                        idle_time = Duration::default();
                    }
                }
            });
        }
    }

    /// Update throughput metrics
    fn update_throughput_metrics(&self, task_count: usize, elapsed: Duration) {
        if elapsed.as_millis() > 0 {
            let throughput = (task_count as u64 * 1000) / elapsed.as_millis() as u64;
            self.metrics.tasks_per_second.store(throughput, Ordering::Relaxed);
        }

        let avg_time = elapsed.as_nanos() as u64 / task_count as u64;
        self.metrics.average_task_time.store(avg_time, Ordering::Relaxed);
    }

    /// Get parallel efficiency (0.0 to 1.0)
    pub fn parallel_efficiency(&self) -> f64 {
        self.metrics.overall_utilization() * self.scheduler.steal_efficiency()
    }

    /// Get performance statistics
    pub fn performance_stats(&self) -> ParallelPerformanceStats {
        ParallelPerformanceStats {
            tasks_submitted: self.scheduler.tasks_submitted.load(Ordering::Relaxed),
            tasks_completed: self.scheduler.tasks_completed.load(Ordering::Relaxed),
            throughput: self.metrics.tasks_per_second.load(Ordering::Relaxed),
            average_task_time: Duration::from_nanos(self.metrics.average_task_time.load(Ordering::Relaxed)),
            worker_utilization: self.metrics.overall_utilization(),
            steal_efficiency: self.scheduler.steal_efficiency(),
            parallel_efficiency: self.parallel_efficiency(),
        }
    }
}

/// Performance statistics snapshot
#[derive(Debug, Clone)]
pub struct ParallelPerformanceStats {
    pub tasks_submitted: u64,
    pub tasks_completed: u64,
    pub throughput: u64,
    pub average_task_time: Duration,
    pub worker_utilization: f64,
    pub steal_efficiency: f64,
    pub parallel_efficiency: f64,
}

/// Estimate the computational cost of translating a term
fn estimate_translation_cost(term: &Term) -> u32 {
    match term {
        Term::Universe(_) | Term::Var(_) | Term::Meta(_) => 1,
        Term::App(f, a) => 5 + estimate_translation_cost(f) + estimate_translation_cost(a),
        Term::Lambda(body) => 10 + estimate_translation_cost(body),
        Term::Pi(ty, body) => 10 + estimate_translation_cost(ty) + estimate_translation_cost(body),
        Term::Let(def, body) => 15 + estimate_translation_cost(def) + estimate_translation_cost(body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Term, Name};

    #[test]
    fn test_task_creation() {
        let term = Term::Universe(Level(0));
        let task = TranslationTask::new(1, term)
            .with_priority(TaskPriority::High)
            .with_dependencies(vec![]);

        assert_eq!(task.id, 1);
        assert_eq!(task.priority, TaskPriority::High);
        assert!(task.dependencies.is_empty());
    }

    #[test]
    fn test_work_stealing_scheduler() {
        let scheduler = WorkStealingScheduler::new(Some(2));

        let task = TranslationTask::new(1, Term::Universe(Level(0)));
        scheduler.submit_task(task);

        assert_eq!(scheduler.tasks_submitted.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_translation_cost_estimation() {
        use std::rc::Rc;

        assert_eq!(estimate_translation_cost(&Term::Universe(Level(0))), 1);
        assert_eq!(estimate_translation_cost(&Term::Var(0)), 1);

        let app = Term::App(
            Rc::new(Term::Universe(Level(0))),
            Rc::new(Term::Var(0))
        );
        assert_eq!(estimate_translation_cost(&app), 7); // 5 + 1 + 1
    }

    #[test]
    fn test_parallel_metrics() {
        let metrics = ParallelMetrics::new(4);

        metrics.worker_utilization[0].store(8500, Ordering::Relaxed); // 85%
        assert_eq!(metrics.worker_utilization_rate(0), 0.85);
    }
}