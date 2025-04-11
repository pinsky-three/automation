use crate::models::action_result::ActionResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TaskState {
    pub status: String, // "in_progress", "completed", "paused", "failed", "task_done"
    pub attempts: u32,  // Number of attempts made
    pub last_action: String, // Last action taken
    pub success_criteria: Vec<String>, // Criteria for task completion
    pub memory: HashMap<String, String>, // Persistent memory across iterations
    pub feedback: Vec<String>, // Feedback from previous attempts
    pub start_time: i64, // Unix timestamp when task started
    pub last_update: i64, // Unix timestamp of last update
    pub action_results: Vec<ActionResult>, // Results of previous actions
    pub analysis: serde_json::Value,
}

impl TaskState {
    pub fn new() -> Self {
        TaskState {
            status: "in_progress".to_string(),
            attempts: 0,
            last_action: String::new(),
            success_criteria: vec![
                "Task completed".to_string(),
                "Information found".to_string(),
                "Research complete".to_string(),
                "Task done".to_string(),
            ],
            memory: HashMap::new(),
            feedback: Vec::new(),
            start_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            last_update: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            action_results: Vec::new(),
            analysis: serde_json::json!({}),
        }
    }

    pub fn update(&mut self, analysis: &str, actions: &str) {
        self.attempts += 1;
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Parse the last action from actions JSON
        if let Ok(actions_json) = serde_json::from_str::<Vec<serde_json::Value>>(actions) {
            if let Some(last_action) = actions_json.last() {
                if let Some(action_type) = last_action["action"].as_str() {
                    self.last_action = action_type.to_string();
                }
            }
        }

        // Update memory based on analysis
        if let Ok(analysis_json) = serde_json::from_str::<serde_json::Value>(analysis) {
            self.analysis = analysis_json.clone();
            if let Some(context) = analysis_json["context"].as_str() {
                self.memory
                    .insert("last_context".to_string(), context.to_string());
            }
            if let Some(state) = analysis_json["state"].as_object() {
                if let Some(window_title) = state["window_title"].as_str() {
                    self.memory
                        .insert("last_window".to_string(), window_title.to_string());
                }
            }
            // Add feedback from challenges
            if let Some(challenges) = analysis_json["challenges"].as_array() {
                for challenge in challenges {
                    if let Some(challenge_str) = challenge.as_str() {
                        self.feedback.push(challenge_str.to_string());
                    }
                }
            }
        }
    }

    pub fn should_pause(&self) -> bool {
        // Pause if too many attempts
        if self.attempts > 10 {
            return true;
        }

        // Pause if stuck in a loop (same action repeated)
        if self.attempts > 3 {
            let last_actions: Vec<String> = self
                .feedback
                .iter()
                .rev()
                .take(3)
                .filter_map(|f| f.split(":").next().map(|s| s.to_string()))
                .collect();

            if last_actions.len() == 3
                && last_actions[0] == last_actions[1]
                && last_actions[1] == last_actions[2]
            {
                return true;
            }
        }

        false
    }

    pub fn is_complete(&self, analysis: &str) -> bool {
        // Don't complete if we haven't taken any actions yet
        if self.attempts < 2 {
            return false;
        }

        // Check if all success criteria are met
        for criterion in &self.success_criteria {
            if !analysis.contains(criterion) {
                return false;
            }
        }

        // Check if there are no challenges
        if let Ok(analysis_json) = serde_json::from_str::<serde_json::Value>(analysis) {
            if let Some(challenges) = analysis_json["challenges"].as_array() {
                if !challenges.is_empty() {
                    return false;
                }
            }
        }

        // Check if we have performed meaningful actions
        if self.last_action.is_empty() {
            return false;
        }

        true
    }

    pub fn set_task_done(&mut self) {
        self.status = "task_done".to_string();
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
    }

    pub fn add_action_result(&mut self, result: ActionResult) {
        self.action_results.push(result);
        self.update("", "");
    }
}
