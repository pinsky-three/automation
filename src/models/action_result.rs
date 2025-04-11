use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActionResult {
    pub action_type: String,
    pub success: bool,
    pub timestamp: i64,
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
                .as_secs() as i64,
            error_message: None,
            retry_count: 0,
        }
    }

    pub fn mark_success(&mut self) {
        self.success = true;
        self.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
    }

    pub fn mark_error(&mut self, error: &str) {
        self.success = false;
        self.error_message = Some(error.to_string());
        self.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
        self.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
    }

    pub fn success(mut self) -> Self {
        self.mark_success();
        self
    }

    pub fn with_error(mut self, error: &str) -> Self {
        self.mark_error(error);
        self
    }
}
