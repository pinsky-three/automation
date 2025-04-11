use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub status: String,
    pub last_update: u64,
    pub analysis: serde_json::Value,
    pub action_results: Vec<ActionResult>,
}

impl TaskState {
    pub fn new() -> Self {
        TaskState {
            status: "idle".to_string(),
            last_update: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            analysis: serde_json::json!({}),
            action_results: Vec::new(),
        }
    }

    pub fn update(&mut self, analysis: serde_json::Value) {
        self.analysis = analysis;
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    pub fn should_pause(&self) -> bool {
        self.status == "paused"
    }

    pub fn is_complete(&self) -> bool {
        self.status == "completed" || self.status == "task_done"
    }

    pub fn set_task_done(&mut self) {
        self.status = "task_done".to_string();
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    pub fn add_action_result(&mut self, result: ActionResult) {
        self.action_results.push(result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub action_type: String,
    pub success: bool,
    pub timestamp: u64,
    pub error_message: Option<String>,
    pub retry_count: u32,
}

impl ActionResult {
    pub fn new(action_type: &str) -> Self {
        ActionResult {
            action_type: action_type.to_string(),
            success: false,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            error_message: None,
            retry_count: 0,
        }
    }

    pub fn mark_success(&mut self) {
        self.success = true;
        self.error_message = None;
    }

    pub fn mark_error(&mut self, message: &str) {
        self.success = false;
        self.error_message = Some(message.to_string());
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}
