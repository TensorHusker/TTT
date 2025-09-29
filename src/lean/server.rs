//! Lean Server Communication Interface
//!
//! This module provides a high-level interface for communicating with Lean 4
//! processes, managing server lifecycle, handling LSP-style communication,
//! and providing robust error handling and recovery mechanisms.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child as TokioChild, Command};
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::time::timeout;

use crate::lean::{LeanTerm, LeanName, LeanLevel, LeanError, Result as LeanResult};

/// Server-specific errors
#[derive(Error, Debug)]
pub enum ServerError {
    #[error("Lean server not running")]
    ServerNotRunning,

    #[error("Failed to start Lean server: {reason}")]
    StartupFailed { reason: String },

    #[error("Communication timeout after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    #[error("Protocol error: {message}")]
    ProtocolError { message: String },

    #[error("Server crashed: {exit_code:?}")]
    ServerCrashed { exit_code: Option<i32> },

    #[error("Invalid response: {response}")]
    InvalidResponse { response: String },

    #[error("Request failed: {error}")]
    RequestFailed { error: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Lean error: {0}")]
    Lean(#[from] LeanError),
}

pub type ServerResult<T> = std::result::Result<T, ServerError>;

/// LSP-style message for communication with Lean server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspMessage {
    /// Request/response ID
    pub id: Option<u64>,

    /// Method name for requests
    pub method: Option<String>,

    /// Parameters for requests
    pub params: Option<Value>,

    /// Result for responses
    pub result: Option<Value>,

    /// Error for error responses
    pub error: Option<LspError>,

    /// JSON-RPC version
    pub jsonrpc: String,
}

impl LspMessage {
    /// Create a new request message
    pub fn request(id: u64, method: String, params: Value) -> Self {
        Self {
            id: Some(id),
            method: Some(method),
            params: Some(params),
            result: None,
            error: None,
            jsonrpc: "2.0".to_string(),
        }
    }

    /// Create a new response message
    pub fn response(id: u64, result: Value) -> Self {
        Self {
            id: Some(id),
            method: None,
            params: None,
            result: Some(result),
            error: None,
            jsonrpc: "2.0".to_string(),
        }
    }

    /// Create a new error response
    pub fn error_response(id: u64, error: LspError) -> Self {
        Self {
            id: Some(id),
            method: None,
            params: None,
            result: None,
            error: Some(error),
            jsonrpc: "2.0".to_string(),
        }
    }

    /// Create a notification (no ID)
    pub fn notification(method: String, params: Value) -> Self {
        Self {
            id: None,
            method: Some(method),
            params: Some(params),
            result: None,
            error: None,
            jsonrpc: "2.0".to_string(),
        }
    }
}

/// LSP error structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

/// Configuration for Lean server
#[derive(Debug, Clone)]
pub struct LeanServerConfig {
    /// Path to Lean executable
    pub lean_executable: PathBuf,

    /// Working directory for Lean process
    pub working_dir: PathBuf,

    /// Additional command line arguments
    pub args: Vec<String>,

    /// Environment variables
    pub env: HashMap<String, String>,

    /// Request timeout
    pub request_timeout: Duration,

    /// Server startup timeout
    pub startup_timeout: Duration,

    /// Maximum concurrent requests
    pub max_concurrent_requests: usize,

    /// Whether to auto-restart on crash
    pub auto_restart: bool,

    /// Maximum restart attempts
    pub max_restart_attempts: u32,

    /// Heartbeat interval
    pub heartbeat_interval: Duration,
}

impl Default for LeanServerConfig {
    fn default() -> Self {
        Self {
            lean_executable: PathBuf::from("lean"),
            working_dir: PathBuf::from("."),
            args: vec!["--server".to_string()],
            env: HashMap::new(),
            request_timeout: Duration::from_secs(30),
            startup_timeout: Duration::from_secs(10),
            max_concurrent_requests: 10,
            auto_restart: true,
            max_restart_attempts: 3,
            heartbeat_interval: Duration::from_secs(30),
        }
    }
}

/// Request tracking information
#[derive(Debug)]
struct PendingRequest {
    response_tx: oneshot::Sender<ServerResult<Value>>,
    created_at: Instant,
    method: String,
}

/// Server metrics and statistics
#[derive(Debug, Default)]
pub struct ServerMetrics {
    pub requests_sent: std::sync::atomic::AtomicU64,
    pub responses_received: std::sync::atomic::AtomicU64,
    pub errors_received: std::sync::atomic::AtomicU64,
    pub timeouts: std::sync::atomic::AtomicU64,
    pub server_restarts: std::sync::atomic::AtomicU32,
    pub avg_response_time_ms: std::sync::atomic::AtomicU64,
    pub uptime_start: std::sync::Mutex<Option<Instant>>,
}

impl ServerMetrics {
    pub fn record_request(&self) {
        self.requests_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_response(&self, duration: Duration) {
        self.responses_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Update average response time
        let old_avg = self.avg_response_time_ms.load(std::sync::atomic::Ordering::Relaxed);
        let responses = self.responses_received.load(std::sync::atomic::Ordering::Relaxed);
        let new_avg = (old_avg * (responses - 1) + duration.as_millis() as u64) / responses;
        self.avg_response_time_ms.store(new_avg, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.errors_received.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_timeout(&self) {
        self.timeouts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn record_restart(&self) {
        self.server_restarts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut uptime_start = self.uptime_start.lock().unwrap();
        *uptime_start = Some(Instant::now());
    }

    pub fn success_rate(&self) -> f64 {
        let total = self.requests_sent.load(std::sync::atomic::Ordering::Relaxed);
        let errors = self.errors_received.load(std::sync::atomic::Ordering::Relaxed) +
                    self.timeouts.load(std::sync::atomic::Ordering::Relaxed);
        if total > 0 {
            1.0 - (errors as f64 / total as f64)
        } else {
            0.0
        }
    }

    pub fn uptime(&self) -> Option<Duration> {
        let uptime_start = self.uptime_start.lock().unwrap();
        uptime_start.as_ref().map(|start| start.elapsed())
    }
}

/// Main Lean server interface
pub struct LeanServer {
    /// Server configuration
    config: LeanServerConfig,

    /// Child process handle
    process: Arc<Mutex<Option<TokioChild>>>,

    /// Request ID counter
    request_id: std::sync::atomic::AtomicU64,

    /// Pending requests
    pending_requests: Arc<DashMap<u64, PendingRequest>>,

    /// Communication channels
    message_tx: mpsc::UnboundedSender<LspMessage>,

    /// Server metrics
    metrics: Arc<ServerMetrics>,

    /// Concurrency control
    semaphore: Arc<Semaphore>,

    /// Server state
    state: Arc<RwLock<ServerState>>,

    /// Restart counter
    restart_count: std::sync::atomic::AtomicU32,
}

#[derive(Debug, Clone, PartialEq)]
enum ServerState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
}

impl LeanServer {
    /// Create a new Lean server instance
    pub async fn new(config: LeanServerConfig) -> ServerResult<Self> {
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        let server = Self {
            config: config.clone(),
            process: Arc::new(Mutex::new(None)),
            request_id: std::sync::atomic::AtomicU64::new(1),
            pending_requests: Arc::new(DashMap::new()),
            message_tx,
            metrics: Arc::new(ServerMetrics::default()),
            semaphore: Arc::new(Semaphore::new(config.max_concurrent_requests)),
            state: Arc::new(RwLock::new(ServerState::Stopped)),
            restart_count: std::sync::atomic::AtomicU32::new(0),
        };

        // Start the server
        server.start().await?;

        // Start message processing task
        server.start_message_processor(message_rx).await;

        // Start heartbeat task if configured
        if config.heartbeat_interval > Duration::ZERO {
            server.start_heartbeat_task().await;
        }

        Ok(server)
    }

    /// Start the Lean server process
    pub async fn start(&self) -> ServerResult<()> {
        let mut state = self.state.write();
        if *state == ServerState::Running {
            return Ok(());
        }

        *state = ServerState::Starting;
        drop(state);

        tracing::info!("Starting Lean server: {:?}", self.config.lean_executable);

        let mut cmd = Command::new(&self.config.lean_executable);
        cmd.args(&self.config.args)
           .current_dir(&self.config.working_dir)
           .stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped())
           .kill_on_drop(true);

        // Set environment variables
        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        let child = cmd.spawn()
            .map_err(|e| ServerError::StartupFailed { reason: e.to_string() })?;

        {
            let mut process = self.process.lock();
            *process = Some(child);
        }

        // Wait for server to be ready
        self.wait_for_ready().await?;

        let mut state = self.state.write();
        *state = ServerState::Running;

        self.metrics.record_restart();
        tracing::info!("Lean server started successfully");

        Ok(())
    }

    /// Stop the Lean server
    pub async fn stop(&self) -> ServerResult<()> {
        let mut state = self.state.write();
        if *state == ServerState::Stopped {
            return Ok(());
        }

        *state = ServerState::Stopping;
        drop(state);

        tracing::info!("Stopping Lean server");

        let mut process = self.process.lock();
        if let Some(mut child) = process.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        let mut state = self.state.write();
        *state = ServerState::Stopped;

        tracing::info!("Lean server stopped");
        Ok(())
    }

    /// Restart the Lean server
    pub async fn restart(&self) -> ServerResult<()> {
        let restart_count = self.restart_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if restart_count >= self.config.max_restart_attempts {
            return Err(ServerError::StartupFailed {
                reason: "Maximum restart attempts exceeded".to_string(),
            });
        }

        tracing::warn!("Restarting Lean server (attempt {})", restart_count + 1);

        self.stop().await?;

        // Clear pending requests with error
        self.clear_pending_requests().await;

        // Wait a bit before restarting
        tokio::time::sleep(Duration::from_millis(1000)).await;

        self.start().await?;

        Ok(())
    }

    /// Send a request to the Lean server
    pub async fn send_request(&self, method: String, params: Value) -> ServerResult<Value> {
        // Check if server is running
        {
            let state = self.state.read();
            if *state != ServerState::Running {
                return Err(ServerError::ServerNotRunning);
            }
        }

        // Acquire semaphore permit
        let _permit = self.semaphore.acquire().await.unwrap();

        let request_id = self.request_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (response_tx, response_rx) = oneshot::channel();

        // Store pending request
        let pending = PendingRequest {
            response_tx,
            created_at: Instant::now(),
            method: method.clone(),
        };
        self.pending_requests.insert(request_id, pending);

        // Create and send message
        let message = LspMessage::request(request_id, method, params);
        self.message_tx.send(message)
            .map_err(|_| ServerError::ServerNotRunning)?;

        self.metrics.record_request();

        // Wait for response with timeout
        match timeout(self.config.request_timeout, response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                self.pending_requests.remove(&request_id);
                Err(ServerError::ServerNotRunning)
            }
            Err(_) => {
                self.pending_requests.remove(&request_id);
                self.metrics.record_timeout();
                Err(ServerError::Timeout {
                    duration_ms: self.config.request_timeout.as_millis() as u64,
                })
            }
        }
    }

    /// Type check a Lean term
    pub async fn type_check(&self, term: LeanTerm) -> ServerResult<LeanTerm> {
        let params = json!({
            "term": self.term_to_string(&term),
        });

        let result = self.send_request("typeCheck".to_string(), params).await?;
        self.parse_type_result(&result)
    }

    /// Apply a tactic to a goal
    pub async fn apply_tactic(&self, goal: LeanTerm, tactic: String) -> ServerResult<LeanTerm> {
        let params = json!({
            "goal": self.term_to_string(&goal),
            "tactic": tactic,
        });

        let result = self.send_request("applyTactic".to_string(), params).await?;
        self.parse_proof_result(&result)
    }

    /// Normalize a Lean term
    pub async fn normalize(&self, term: LeanTerm) -> ServerResult<LeanTerm> {
        let params = json!({
            "term": self.term_to_string(&term),
        });

        let result = self.send_request("normalize".to_string(), params).await?;
        self.parse_term_result(&result)
    }

    /// Check if the server is alive
    pub async fn ping(&self) -> ServerResult<()> {
        let params = json!({});
        let _result = self.send_request("ping".to_string(), params).await?;
        Ok(())
    }

    /// Get server information
    pub async fn get_server_info(&self) -> ServerResult<ServerInfo> {
        let params = json!({});
        let result = self.send_request("serverInfo".to_string(), params).await?;

        Ok(ServerInfo {
            version: result.get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            commit: result.get("commit")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            build_date: result.get("buildDate")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    /// Convert LeanTerm to string representation for server communication
    fn term_to_string(&self, term: &LeanTerm) -> String {
        // TODO: Implement proper Lean syntax serialization
        format!("{}", term)
    }

    /// Parse type checking result
    fn parse_type_result(&self, result: &Value) -> ServerResult<LeanTerm> {
        // TODO: Implement proper parsing from Lean server response
        if let Some(type_str) = result.get("type").and_then(|v| v.as_str()) {
            self.parse_lean_term(type_str)
        } else {
            Err(ServerError::InvalidResponse {
                response: result.to_string(),
            })
        }
    }

    /// Parse proof result
    fn parse_proof_result(&self, result: &Value) -> ServerResult<LeanTerm> {
        // TODO: Implement proper parsing from Lean server response
        if let Some(proof_str) = result.get("proof").and_then(|v| v.as_str()) {
            self.parse_lean_term(proof_str)
        } else {
            Err(ServerError::InvalidResponse {
                response: result.to_string(),
            })
        }
    }

    /// Parse term result
    fn parse_term_result(&self, result: &Value) -> ServerResult<LeanTerm> {
        // TODO: Implement proper parsing from Lean server response
        if let Some(term_str) = result.get("term").and_then(|v| v.as_str()) {
            self.parse_lean_term(term_str)
        } else {
            Err(ServerError::InvalidResponse {
                response: result.to_string(),
            })
        }
    }

    /// Parse Lean term from string (placeholder implementation)
    fn parse_lean_term(&self, term_str: &str) -> ServerResult<LeanTerm> {
        // TODO: Implement proper Lean term parsing
        // For now, return a simple constant
        Ok(LeanTerm::const_(term_str))
    }

    /// Wait for server to be ready
    async fn wait_for_ready(&self) -> ServerResult<()> {
        let start = Instant::now();

        while start.elapsed() < self.config.startup_timeout {
            match self.ping().await {
                Ok(_) => return Ok(()),
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }

        Err(ServerError::StartupFailed {
            reason: "Server failed to respond to ping".to_string(),
        })
    }

    /// Start message processing task
    async fn start_message_processor(&self, mut message_rx: mpsc::UnboundedReceiver<LspMessage>) {
        let process = self.process.clone();
        let pending_requests = self.pending_requests.clone();
        let metrics = self.metrics.clone();
        let state = self.state.clone();

        tokio::spawn(async move {
            // Get process handles
            let (mut stdin, mut stdout) = {
                let process_guard = process.lock();
                if let Some(ref mut child) = *process_guard {
                    let stdin = child.stdin.take()
                        .ok_or_else(|| ServerError::StartupFailed {
                            reason: "Failed to get stdin handle".to_string(),
                        });
                    let stdout = child.stdout.take()
                        .ok_or_else(|| ServerError::StartupFailed {
                            reason: "Failed to get stdout handle".to_string(),
                        });

                    match (stdin, stdout) {
                        (Ok(stdin), Ok(stdout)) => (stdin, stdout),
                        _ => {
                            tracing::error!("Failed to get process handles");
                            return;
                        }
                    }
                } else {
                    tracing::error!("No process available");
                    return;
                }
            };

            let mut writer = BufWriter::new(stdin);
            let mut reader = BufReader::new(stdout);

            // Start response reading task
            let pending_requests_clone = pending_requests.clone();
            let metrics_clone = metrics.clone();
            tokio::spawn(async move {
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            if let Ok(message) = serde_json::from_str::<LspMessage>(&line) {
                                Self::handle_response(message, &pending_requests_clone, &metrics_clone).await;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error reading from server: {}", e);
                            break;
                        }
                    }
                }

                // Server died, mark state as crashed
                let mut state_guard = state.write();
                *state_guard = ServerState::Crashed;
            });

            // Handle outgoing messages
            while let Some(message) = message_rx.recv().await {
                let json_str = match serde_json::to_string(&message) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!("Failed to serialize message: {}", e);
                        continue;
                    }
                };

                let content_length = json_str.len();
                let full_message = format!("Content-Length: {}\r\n\r\n{}", content_length, json_str);

                if let Err(e) = writer.write_all(full_message.as_bytes()).await {
                    tracing::error!("Failed to write to server: {}", e);
                    break;
                }

                if let Err(e) = writer.flush().await {
                    tracing::error!("Failed to flush writer: {}", e);
                    break;
                }
            }
        });
    }

    /// Handle response from server
    async fn handle_response(
        message: LspMessage,
        pending_requests: &DashMap<u64, PendingRequest>,
        metrics: &ServerMetrics,
    ) {
        if let Some(id) = message.id {
            if let Some((_, pending)) = pending_requests.remove(&id) {
                let duration = pending.created_at.elapsed();

                let result = if let Some(error) = message.error {
                    metrics.record_error();
                    Err(ServerError::RequestFailed { error: error.message })
                } else if let Some(result) = message.result {
                    metrics.record_response(duration);
                    Ok(result)
                } else {
                    metrics.record_error();
                    Err(ServerError::InvalidResponse {
                        response: "No result or error in response".to_string(),
                    })
                };

                let _ = pending.response_tx.send(result);
            }
        }
    }

    /// Start heartbeat task
    async fn start_heartbeat_task(&self) {
        let server = self.clone_for_heartbeat();
        let interval = self.config.heartbeat_interval;

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                // Check if server is supposed to be running
                {
                    let state = server.state.read();
                    if *state != ServerState::Running {
                        continue;
                    }
                }

                // Send ping
                match server.ping().await {
                    Ok(_) => {
                        // Server is alive
                    }
                    Err(_) => {
                        // Server is not responding, try to restart if auto-restart is enabled
                        if server.config.auto_restart {
                            tracing::warn!("Server not responding to heartbeat, attempting restart");
                            if let Err(e) = server.restart().await {
                                tracing::error!("Failed to restart server: {}", e);
                            }
                        }
                    }
                }
            }
        });
    }

    /// Clear all pending requests with error
    async fn clear_pending_requests(&self) {
        let error = ServerError::ServerCrashed { exit_code: None };

        // Collect all pending requests
        let pending_ids: Vec<u64> = self.pending_requests.iter()
            .map(|entry| *entry.key())
            .collect();

        // Send error to all pending requests
        for id in pending_ids {
            if let Some((_, pending)) = self.pending_requests.remove(&id) {
                let _ = pending.response_tx.send(Err(error.clone()));
            }
        }
    }

    /// Clone for heartbeat task (simplified clone)
    fn clone_for_heartbeat(&self) -> Self {
        Self {
            config: self.config.clone(),
            process: self.process.clone(),
            request_id: std::sync::atomic::AtomicU64::new(0),
            pending_requests: Arc::new(DashMap::new()),
            message_tx: self.message_tx.clone(),
            metrics: self.metrics.clone(),
            semaphore: self.semaphore.clone(),
            state: self.state.clone(),
            restart_count: std::sync::atomic::AtomicU32::new(0),
        }
    }

    /// Get server metrics
    pub fn metrics(&self) -> &ServerMetrics {
        &self.metrics
    }

    /// Get current server state
    pub fn state(&self) -> ServerState {
        *self.state.read()
    }

    /// Check if server is running
    pub fn is_running(&self) -> bool {
        *self.state.read() == ServerState::Running
    }
}

/// Server information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub commit: Option<String>,
    pub build_date: Option<String>,
}

impl Drop for LeanServer {
    fn drop(&mut self) {
        // Best effort cleanup
        let process = self.process.clone();
        tokio::spawn(async move {
            let mut process = process.lock();
            if let Some(mut child) = process.take() {
                let _ = child.kill().await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_message_creation() {
        let request = LspMessage::request(1, "test".to_string(), json!({"param": "value"}));
        assert_eq!(request.id, Some(1));
        assert_eq!(request.method, Some("test".to_string()));
        assert!(request.params.is_some());

        let response = LspMessage::response(1, json!({"result": "success"}));
        assert_eq!(response.id, Some(1));
        assert!(response.result.is_some());
        assert!(response.method.is_none());
    }

    #[test]
    fn test_server_config_defaults() {
        let config = LeanServerConfig::default();
        assert_eq!(config.lean_executable, PathBuf::from("lean"));
        assert!(config.request_timeout > Duration::ZERO);
        assert!(config.max_concurrent_requests > 0);
    }

    #[test]
    fn test_server_metrics() {
        let metrics = ServerMetrics::default();
        metrics.record_request();
        metrics.record_response(Duration::from_millis(100));

        assert_eq!(metrics.requests_sent.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(metrics.responses_received.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(metrics.success_rate(), 1.0);
    }

    #[test]
    fn test_server_states() {
        assert_eq!(ServerState::Stopped, ServerState::Stopped);
        assert_ne!(ServerState::Running, ServerState::Stopped);
    }

    #[tokio::test]
    async fn test_pending_request_timeout() {
        let (tx, _rx) = oneshot::channel();
        let pending = PendingRequest {
            response_tx: tx,
            created_at: Instant::now(),
            method: "test".to_string(),
        };

        // Simulate timeout by dropping receiver
        drop(_rx);

        // The response_tx.send() would fail, which is expected behavior
        assert_eq!(pending.method, "test");
    }
}