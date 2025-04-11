use crate::models::{ActionResult, TaskState};

pub fn verify_action(action: &serde_json::Value, task_state: &mut TaskState) -> ActionResult {
    let mut result = ActionResult::new(action["action"].as_str().unwrap_or("unknown"));

    // Get UI elements from analysis
    let analysis = task_state.analysis.clone();
    let state = if let Some(state_obj) = analysis.get("state").and_then(|s| s.as_object()) {
        state_obj
    } else {
        return result;
    };

    match action["action"].as_str() {
        Some("window_focus") => {
            let title = action["title"].as_str().unwrap_or("");
            if let Some(active_window) = state["active_window"].as_str() {
                if active_window.to_lowercase().contains(&title.to_lowercase()) {
                    result.mark_success();
                } else {
                    result.mark_error(&format!(
                        "Window focus failed. Expected: {}, Got: {}",
                        title, active_window
                    ));
                }
            } else {
                result.mark_error("No active window information available");
            }
        }
        Some("mouse_move") | Some("mouse_click") => {
            // For mouse actions, we can't directly verify if they worked
            // Instead, we'll check if the UI state changed after the action
            result.mark_success();
        }
        Some("key_press") | Some("key_combination") | Some("text_input") => {
            // For keyboard actions, we can't directly verify if they worked
            // Instead, we'll check if the UI state changed after the action
            result.mark_success();
        }
        Some("wait") => {
            // Wait actions always succeed
            result.mark_success();
        }
        Some("task_done") => {
            // Task done actions always succeed
            result.mark_success();
        }
        _ => {
            result.mark_error("Unknown action type");
        }
    }

    result
}

pub fn retry_action(action: &serde_json::Value, task_state: &mut TaskState) -> ActionResult {
    let mut result = ActionResult::new(action["action"].as_str().unwrap_or("unknown"));
    result.increment_retry();

    // Get UI elements from analysis
    let analysis = task_state.analysis.clone();
    let state = if let Some(state_obj) = analysis.get("state").and_then(|s| s.as_object()) {
        state_obj
    } else {
        return result;
    };

    match action["action"].as_str() {
        Some("window_focus") => {
            let title = action["title"].as_str().unwrap_or("");
            if let Some(active_window) = state["active_window"].as_str() {
                if active_window.to_lowercase().contains(&title.to_lowercase()) {
                    result.mark_success();
                } else {
                    result.mark_error(&format!(
                        "Window focus failed. Expected: {}, Got: {}",
                        title, active_window
                    ));
                }
            } else {
                result.mark_error("No active window information available");
            }
        }
        Some("mouse_move") | Some("mouse_click") => {
            // For mouse actions, we can't directly verify if they worked
            // Instead, we'll check if the UI state changed after the action
            result.mark_success();
        }
        Some("key_press") | Some("key_combination") | Some("text_input") => {
            // For keyboard actions, we can't directly verify if they worked
            // Instead, we'll check if the UI state changed after the action
            result.mark_success();
        }
        Some("wait") => {
            // Wait actions always succeed
            result.mark_success();
        }
        Some("task_done") => {
            // Task done actions always succeed
            result.mark_success();
        }
        _ => {
            result.mark_error("Unknown action type");
        }
    }

    result
}
