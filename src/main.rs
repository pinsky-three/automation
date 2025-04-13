mod actuators;

use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestMessageContentPartImageArgs,
    ChatCompletionRequestMessageContentPartText, ChatCompletionRequestMessageContentPartTextArgs,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, ChatCompletionRequestUserMessageContentPart,
    CreateChatCompletionRequestArgs, ImageDetail, ImageUrlArgs,
};
use automation::senses::screens::Screens;
use base64::Engine;
use chrono::Local;
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use fs_extra::dir;
use image::imageops::FilterType;
use image::{GenericImageView, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs};
use std::{thread::sleep, time::Duration};

// Import our custom keyboard
use crate::actuators::keyboard::Keyboard as CustomKeyboard;
use crate::actuators::mouse::Mouse as CustomMouse;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TaskState {
    instruction: String,               // The instruction for the task
    status: String,      // "in_progress", "completed", "paused", "failed", "task_done"
    attempts: u32,       // Number of attempts made
    last_action: String, // Last action taken
    success_criteria: Vec<String>, // Criteria for task completion
    memory: HashMap<String, String>, // Persistent memory across iterations
    feedback: Vec<String>, // Feedback from previous attempts
    start_time: i64,     // Unix timestamp when task started
    last_update: i64,    // Unix timestamp of last update
    action_results: Vec<ActionResult>, // Results of previous actions
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ActionResult {
    action_type: String,           // Type of action performed
    success: bool,                 // Whether the action was successful
    timestamp: i64,                // When the action was performed
    error_message: Option<String>, // Error message if the action failed
    retry_count: u32,              // Number of retries attempted
}

impl ActionResult {
    fn new(action_type: &str) -> Self {
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

    fn success(mut self) -> Self {
        self.success = true;
        self
    }

    fn with_error(mut self, error: &str) -> Self {
        self.error_message = Some(error.to_string());
        self
    }

    fn increment_retry(mut self) -> Self {
        self.retry_count += 1;
        self
    }
}

impl TaskState {
    fn new(instruction: String) -> Self {
        TaskState {
            instruction,
            status: "in_progress".to_string(),
            attempts: 0,
            last_action: String::new(),
            success_criteria: vec![
                "Task completed".to_string(),
                "Information found".to_string(),
                "Research complete".to_string(),
                "Task done".to_string(), // Added new success criterion
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
        }
    }

    fn update(&mut self, analysis: &str, actions: &str) {
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

    fn should_pause(&self) -> bool {
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

    fn is_complete(&self, analysis: &str) -> bool {
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

    // New method to explicitly set task to done state
    fn set_task_done(&mut self) {
        self.status = "task_done".to_string();
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
    }
}

// Function to load or create task state
fn load_task_state(iteration_dir: &str) -> TaskState {
    let state_path = Path::new(iteration_dir).join("task_state.json");
    if state_path.exists() {
        if let Ok(state_json) = fs::read_to_string(&state_path) {
            if let Ok(state) = serde_json::from_str::<TaskState>(&state_json) {
                return state;
            }
        }
    }
    TaskState::new(String::new())
}

// Function to save task state
fn save_task_state(iteration_dir: &str, state: &TaskState) {
    let state_path = Path::new(iteration_dir).join("task_state.json");
    if let Ok(state_json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(&state_path, state_json);
    }
}

// Function to get the last N iterations
// fn get_last_n_iterations(n: usize) -> Vec<(String, String, String)> {
//     let iterations_dir = Path::new("target/iterations");
//     if !iterations_dir.exists() {
//         return Vec::new();
//     }

//     let mut iterations: Vec<(String, String, String)> = Vec::new();

//     // Get all iteration directories and sort them by name (timestamp) in descending order
//     let mut dirs: Vec<_> = fs::read_dir(iterations_dir)
//         .unwrap()
//         .filter_map(|entry| entry.ok())
//         .filter(|entry| entry.path().is_dir())
//         .collect();
//     dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

//     // Take the last N iterations
//     for entry in dirs.iter().take(n) {
//         let dir_path = entry.path();
//         let metadata_path = dir_path.join("metadata.json");
//         let analysis_path = dir_path.join("analysis.json");
//         let actions_path = dir_path.join("actions.json");

//         if metadata_path.exists() && analysis_path.exists() && actions_path.exists() {
//             if let (Ok(metadata), Ok(analysis), Ok(actions)) = (
//                 fs::read_to_string(&metadata_path),
//                 fs::read_to_string(&analysis_path),
//                 fs::read_to_string(&actions_path),
//             ) {
//                 iterations.push((metadata, analysis, actions));
//             }
//         }
//     }

//     iterations
// }

// // Function to get screenshot from iteration directory
// fn get_screenshot_from_iteration(dir_path: &Path) -> Option<String> {
//     let screenshot_path = dir_path.join("screenshot_resized.png");
//     if screenshot_path.exists() {
//         if let Ok(img) = ImageReader::open(&screenshot_path) {
//             if let Ok(img) = img.decode() {
//                 let (w, h) = img.dimensions();
//                 let img = img.resize(w / 3, h / 3, FilterType::CatmullRom);

//                 // Create a buffer to store the image data
//                 let mut buf = Vec::new();
//                 let mut cursor = std::io::Cursor::new(&mut buf);
//                 if img.write_to(&mut cursor, ImageFormat::Png).is_ok() {
//                     // Encode the image data to base64
//                     return Some(base64::engine::general_purpose::STANDARD.encode(&buf));
//                 }
//             }
//         }
//     }
//     None
// }

// Function to get the last N iterations with screenshots
// fn get_last_n_iterations_with_screenshots(
//     n: usize,
// ) -> Vec<(String, String, String, Option<String>)> {
//     let iterations_dir = Path::new("target/iterations");
//     if !iterations_dir.exists() {
//         return Vec::new();
//     }

//     let mut iterations: Vec<(String, String, String, Option<String>)> = Vec::new();

//     // Get all iteration directories and sort them by name (timestamp) in descending order
//     let mut dirs: Vec<_> = fs::read_dir(iterations_dir)
//         .unwrap()
//         .filter_map(|entry| entry.ok())
//         .filter(|entry| entry.path().is_dir())
//         .collect();

//     dirs.sort_by_key(|b| std::cmp::Reverse(b.file_name()));

//     // Take the last N iterations
//     for entry in dirs.iter().take(n) {
//         let dir_path = entry.path();
//         let metadata_path = dir_path.join("metadata.json");
//         let analysis_path = dir_path.join("analysis.json");
//         let actions_path = dir_path.join("actions.json");

//         if metadata_path.exists() && analysis_path.exists() && actions_path.exists() {
//             if let (Ok(metadata), Ok(analysis), Ok(actions)) = (
//                 fs::read_to_string(&metadata_path),
//                 fs::read_to_string(&analysis_path),
//                 fs::read_to_string(&actions_path),
//             ) {
//                 let screenshot = get_screenshot_from_iteration(&dir_path);
//                 iterations.push((metadata, analysis, actions, screenshot));
//             }
//         }
//     }

//     iterations
// }

// Function to format iterations history for the prompt
// fn format_iterations_history(iterations: &[(String, String, String, Option<String>)]) -> String {
//     if iterations.is_empty() {
//         return String::from("No previous iterations available.");
//     }

//     let mut history = String::from("Previous iterations:\n\n");

//     for (metadata, analysis, actions, _) in iterations {
//         if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata) {
//             if let (Some(timestamp), Some(instruction), Some(status)) = (
//                 meta["timestamp"].as_str(),
//                 meta["instruction"].as_str(),
//                 meta["status"].as_str(),
//             ) {
//                 history.push_str(&format!("Iteration {}:\n", timestamp));
//                 history.push_str(&format!("Instruction: {}\n", instruction));
//                 history.push_str(&format!("Status: {}\n", status));
//                 if let Some(feedback) = meta["feedback"].as_str() {
//                     history.push_str(&format!("Feedback: {}\n", feedback));
//                 }
//                 history.push_str("Analysis:\n");
//                 history.push_str(&format!("{}\n", analysis));
//                 history.push_str("Actions:\n");
//                 history.push_str(&format!("{}\n\n", actions));
//             }
//         }
//     }

//     history
// }

// Function to generate self-instruction based on history
async fn generate_self_instruction(
    client: &Client<OpenAIConfig>,
    model_name: &str,
    // history: &[(String, String, String, Option<String>)],
    current_instruction: &str,
    task_state: &TaskState,
    max_tokens: u32,
) -> String {
    // if history.is_empty() {
    //     return current_instruction.to_string();
    // }

    // let history_text = format_iterations_history(history);
    // let last_iteration = &history[0];

    // Use task state for feedback instead of is_task_complete
    let feedback = if task_state.status == "completed" {
        "Task completed successfully".to_string()
    } else if !task_state.feedback.is_empty() {
        task_state.feedback.last().unwrap().clone()
    } else {
        "Task in progress".to_string()
    };

    let self_instruction_request = CreateChatCompletionRequestArgs::default()
        .model(model_name)
        .max_tokens(max_tokens)
        .messages([ChatCompletionRequestUserMessageArgs::default()
            .content(vec![
                ChatCompletionRequestMessageContentPartTextArgs::default()
                    .text(format!(
                        "Based on the following history and feedback, generate a refined instruction to achieve the original goal: '{}'

Feedback from last attempt: {}

Current Task State:
- Status: {}
- Attempts: {}
- Last Action: {}
- Memory: {:?}

Generate a new instruction that:
1. Addresses the feedback from previous attempts
2. Maintains the original goal
3. Is clear and specific
4. Focuses on overcoming identified challenges

Response should be ONLY the new instruction, no additional text.",
                        current_instruction,  feedback,
                        task_state.status,
                        task_state.attempts,
                        task_state.last_action,
                        task_state.memory
                    ))
                    .build()
                    .unwrap()
                    .into()])
            .build()
            .unwrap()
            .into()])
        .build()
        .unwrap();

    let response = client
        .chat()
        .create(self_instruction_request)
        .await
        .unwrap();
    let mut new_instruction = String::new();
    for choice in response.choices {
        new_instruction = choice.message.content.unwrap_or_default();
    }

    new_instruction.trim().to_string()
}

// Function to verify if an action was successful
fn verify_action(
    action: &serde_json::Value,
    analysis_json: &serde_json::Value,
    task_state: &mut TaskState,
) -> ActionResult {
    let mut result = ActionResult::new(action["action"].as_str().unwrap_or("unknown"));

    // Get UI elements from analysis
    let ui_elements = if let Some(elements) = analysis_json["ui_elements"].as_array() {
        elements
    } else {
        result.error_message = Some("No UI elements found in analysis".to_string());
        return result;
    };

    // Get current state from analysis
    let state = if let Some(state_obj) = analysis_json["state"].as_object() {
        state_obj
    } else {
        result.error_message = Some("No state information found in analysis".to_string());
        return result;
    };

    // Verify based on action type
    match action["action"].as_str() {
        Some("window_focus") => {
            if let Some(title) = action["title"].as_str() {
                if let Some(active_window) = state["active_window"].as_str() {
                    if active_window.to_lowercase().contains(&title.to_lowercase()) {
                        result = result.success();
                    } else {
                        result.error_message = Some(format!(
                            "Window focus failed. Expected: {}, Got: {}",
                            title, active_window
                        ));
                    }
                } else {
                    result.error_message = Some("Could not determine active window".to_string());
                }
            } else {
                result.error_message =
                    Some("Missing title or class in window_focus action".to_string());
            }
        }
        Some("mouse_move") | Some("mouse_click") => {
            // For mouse actions, we can't directly verify if they worked
            // Instead, we'll check if the UI state changed after the action
            // This is a simplified approach - in a real implementation, you'd want to
            // compare the UI state before and after the action
            result = result.success();
        }
        Some("key_press") | Some("key_combination") | Some("text_input") => {
            // For keyboard actions, we can't directly verify if they worked
            // Instead, we'll check if the UI state changed after the action
            result = result.success();
        }
        Some("wait") => {
            // Wait actions always succeed
            result = result.success();
        }
        Some("task_done") => {
            // Task done actions always succeed
            result = result.success();
        }
        _ => {
            result.error_message = Some(format!(
                "Unknown action type: {}",
                action["action"].as_str().unwrap_or("unknown")
            ));
        }
    }

    // Add the result to the task state
    task_state.action_results.push(result.clone());

    result
}

// Function to retry a failed action with adjusted parameters
fn retry_action(
    action: &serde_json::Value,
    analysis_json: &serde_json::Value,
    task_state: &mut TaskState,
    enigo: &mut Enigo,
) -> ActionResult {
    let mut result = verify_action(action, analysis_json, task_state);

    // If the action failed and we haven't retried too many times, try again with adjustments
    if !result.success && result.retry_count < 3 {
        result = result.increment_retry();

        // Get the last result for this action type
        let last_result = task_state
            .action_results
            .iter()
            .filter(|r| r.action_type == result.action_type)
            .last();

        // Adjust the action based on the previous failure
        match action["action"].as_str() {
            Some("window_focus") => {
                // Try a different method for window focus
                if let Some(method) = action["method"].as_str() {
                    let new_method = if method == "alt_tab" {
                        "super_tab"
                    } else {
                        "alt_tab"
                    };
                    println!("Retrying window focus with method: {}", new_method);

                    if let (Some(title), Some(class)) =
                        (action["title"].as_str(), action["class"].as_str())
                    {
                        // Create a keyboard handler and use it
                        let mut keyboard = crate::actuators::keyboard::Keyboard::new(enigo);
                        match new_method {
                            "alt_tab" => {
                                let _ = keyboard.alt_tab();
                            }
                            "super_tab" => {
                                let _ = keyboard.cmd_tab();
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some("mouse_move") | Some("mouse_click") => {
                // Adjust mouse coordinates based on UI elements
                if let (Some(x), Some(y)) = (action["x"].as_i64(), action["y"].as_i64()) {
                    // Get UI elements from analysis
                    if let Some(elements) = analysis_json["ui_elements"].as_array() {
                        // Find the closest UI element to the target coordinates
                        let mut closest_element = None;
                        let mut min_distance = f64::MAX;

                        for element in elements {
                            if let Some(coords) = element["coords"].as_array() {
                                if coords.len() >= 4 {
                                    if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
                                        coords[0].as_i64(),
                                        coords[1].as_i64(),
                                        coords[2].as_i64(),
                                        coords[3].as_i64(),
                                    ) {
                                        // Calculate center of the element
                                        let center_x = (x1 + x2) / 2;
                                        let center_y = (y1 + y2) / 2;

                                        // Calculate distance to target
                                        let distance =
                                            ((center_x - x).pow(2) + (center_y - y).pow(2)) as f64;

                                        if distance < min_distance {
                                            min_distance = distance;
                                            closest_element = Some((center_x, center_y));
                                        }
                                    }
                                }
                            }
                        }

                        // If we found a close element, use its coordinates
                        if let Some((new_x, new_y)) = closest_element {
                            println!("Adjusting mouse coordinates to ({}, {})", new_x, new_y);

                            // Move to the new coordinates using our custom Mouse
                            let mut mouse = CustomMouse::new(enigo);
                            let _ = mouse.move_to(new_x as i32, new_y as i32);

                            // If this is a click action, click at the new coordinates
                            if action["action"].as_str() == Some("mouse_click") {
                                if let Some(button) = action["button"].as_str() {
                                    let _ = mouse.click(button);
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                // For other actions, just wait a bit longer and try again
                sleep(Duration::from_millis(500));
            }
        }

        // Verify the action again after retry
        result = verify_action(action, analysis_json, task_state);
    }

    result
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenvy::dotenv().ok();

    let api_base =
        std::env::var("API_BASE").unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());
    let model_name =
        std::env::var("MODEL_NAME").unwrap_or_else(|_| "google/gemini-2.0-flash-001".to_string());
    let max_tokens = std::env::var("MAX_TOKENS")
        .unwrap_or_else(|_| "512".to_string())
        .parse::<u32>()
        .unwrap_or(512);

    println!("max_tokens: {}", max_tokens);

    let api_key = std::env::var("API_KEY").unwrap();

    let client = Client::with_config(
        OpenAIConfig::new()
            .with_api_base(api_base)
            .with_api_key(api_key),
    );

    let mut enigo = Enigo::new(&Settings::default()).unwrap();
    let should_continue = Arc::new(Mutex::new(true));
    let should_continue_clone = should_continue.clone();
    let current_instruction = Arc::new(Mutex::new(String::from("")));
    let current_instruction_clone = current_instruction.clone();
    let is_idle = Arc::new(Mutex::new(true));
    let is_idle_clone = is_idle.clone();

    // Get screen dimensions
    let (screen_width, screen_height) = enigo.main_display().unwrap();
    println!("Screen dimensions: {}x{}", screen_width, screen_height);

    // Spawn a thread to handle user input
    let input_handle = thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut input = String::new();

        println!("Available commands:");
        println!("  stop - Stop the automation");
        println!("  pause - Pause the automation");
        println!("  resume - Resume the automation");
        println!("  help - Show this help message");
        println!("  Any other input will be treated as an instruction for the AI");
        println!(
            "Note: The AI can put itself in a 'task_done' state and wait for new instructions."
        );
        println!("Waiting for your first instruction...");

        while *should_continue_clone.lock().unwrap() {
            print!("> ");
            io::stdout().flush().unwrap();
            input.clear();
            if reader.read_line(&mut input).is_ok() {
                let input = input.trim();
                match input {
                    "stop" => {
                        *should_continue_clone.lock().unwrap() = false;
                        println!("Stopping automation...");
                    }
                    "pause" => {
                        println!("Pausing automation...");
                        // TODO: Implement pause functionality
                    }
                    "resume" => {
                        println!("Resuming automation...");
                        // TODO: Implement resume functionality
                    }
                    "help" => {
                        println!("Available commands:");
                        println!("  stop - Stop the automation");
                        println!("  pause - Pause the automation");
                        println!("  resume - Resume the automation");
                        println!("  help - Show this help message");
                        println!("  Any other input will be treated as an instruction for the AI");
                        println!(
                            "Note: The AI can put itself in a 'task_done' state and wait for new instructions."
                        );
                    }
                    _ => {
                        *current_instruction_clone.lock().unwrap() = input.to_string();
                        *is_idle_clone.lock().unwrap() = false;
                        println!("New instruction set: {}", input);
                    }
                }
            }
        }
    });

    let screens = Screens::new();

    while *should_continue.lock().unwrap() {
        // Check if we're in idle state
        if *is_idle.lock().unwrap() {
            sleep(Duration::from_millis(100));
            continue;
        }

        let start = Instant::now();
        // let monitors = Monitor::all().unwrap();
        // let monitors = screens.get_monitors();

        // screens.report();

        // // Create timestamp for this iteration
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let iteration_dir = format!("target/iterations/{}", timestamp);
        fs::create_dir_all(&iteration_dir).unwrap();

        // // dir::create_all("target/monitors", true).unwrap();

        // let monitor = monitors.first().unwrap();
        // let image = monitor.capture_image().unwrap();

        // let image_file_name = format!("{}/screenshot.png", iteration_dir);
        // image.save(&image_file_name).unwrap();

        // ---

        // let start = Instant::now();

        // let img = ImageReader::open(&image_file_name).unwrap();

        // let img = img.decode().unwrap();

        // let (w, h) = img.dimensions();
        // let img = img.resize(w / 3, h / 3, FilterType::CatmullRom);

        // let resized_image_file_name = format!("{}/screenshot_resized.png", iteration_dir);
        // img.save(&resized_image_file_name).unwrap();

        // // Create a buffer to store the image data
        // let mut buf = Vec::new();
        // let mut cursor = std::io::Cursor::new(&mut buf);
        // img.write_to(&mut cursor, ImageFormat::Png).unwrap();

        // // Encode the image data to base64
        // let res_base64 = base64::engine::general_purpose::STANDARD.encode(&buf);

        // println!("encode time: {:?}", start.elapsed());

        let base64_images = screens.capture_and_save_all_with_base64(&iteration_dir, 3)?;
        // let res_base64 = base64_images.first().unwrap();

        println!("capture and encode time: {:?}", start.elapsed());
        // ---

        let start = Instant::now();

        let instruction = current_instruction.lock().unwrap().clone();

        // Get the last 3 iterations with screenshots for context
        // let iterations_history = get_last_n_iterations_with_screenshots(3);

        // println!("iterations_history (len): {:?}", iterations_history.len());
        // let history_text = format_iterations_history(&iterations_history);

        // let conversation = load_conversation_history(3);

        // Load or create task state
        let mut task_state = load_task_state(&iteration_dir);

        // If we're coming from a task_done state and have a new instruction, reset the task state
        if task_state.status == "task_done" && !instruction.is_empty() {
            println!("Starting new task with instruction: {}", instruction);
            task_state = TaskState::new(instruction.clone());
        }

        // Update task state with current iteration
        // // task_state.update(&history_text, &history_text);

        // // Check if we should pause
        // if task_state.should_pause() {
        //     println!("Task paused due to too many attempts or detected loop");
        //     task_state.status = "paused".to_string();
        //     save_task_state(&iteration_dir, &task_state);
        //     sleep(Duration::from_secs(5));
        //     continue;
        // }

        // // Check if task is complete
        // if task_state.is_complete(&history_text) {
        //     println!(
        //         "Task completed successfully after {} attempts!",
        //         task_state.attempts
        //     );
        //     task_state.status = "completed".to_string();
        //     save_task_state(&iteration_dir, &task_state);
        //     break;
        // }

        // Check if task is in task_done state
        if task_state.status == "task_done" {
            println!("Task is in 'task_done' state. Waiting for new instructions...");
            *is_idle.lock().unwrap() = true;
            save_task_state(&iteration_dir, &task_state);
            continue;
        }

        // Save updated task state
        save_task_state(&iteration_dir, &task_state);

        // Add task state to the prompt
        let state_context = format!(
            "\nTASK STATE:
        - Instruction: {}
        - Status: {}
        - Attempts: {}
        - Last Action: {}
        - Memory: {:?}
        - Feedback: {:?}",
            instruction,
            task_state.status,
            task_state.attempts,
            task_state.last_action,
            task_state.memory,
            task_state.feedback
        );

        let system_message = format!(
        "
            You are a task automation agent.
            You will be given a task to complete.
            You will need to analyze the current state of the task and plan a sequence of actions to complete the task.
            The current timestamp is: {}
            The current operating system is: {}
        ",
            Local::now().format("%Y%m%d_%H%M%S"),
            env::consts::OS
        );

        struct HistoryMessage {
            state_message: String,
            analysis_message: String,
            screens_message: Vec<String>,
            actions_message: String,
        }

        let history_messages = [HistoryMessage {
            state_message: "".to_string(),
            analysis_message: "".to_string(),
            screens_message: vec![],
            actions_message: "".to_string(),
        }];

        let mut complete_chat_messages = vec![];

        complete_chat_messages.push(ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_message)
                .build()
                .unwrap(),
        ));

        history_messages.iter().for_each(|message| {
            let frame = (
                message.state_message.clone(),
                message.screens_message.clone(),
                message.analysis_message.clone(),
            );

            let frames = vec![frame];

            let messages =
                frames
                    .into_iter()
                    .map(|(state_message, screens_message, analysis_message)| {
                        // let mut message = ChatCompletionRequestMessageContentPartText::default();
                        // message.text = state_message.to_string();

                        let images = screens_message.into_iter().map(|screen_message| {
                            ChatCompletionRequestUserMessageContentPart::ImageUrl(
                                ChatCompletionRequestMessageContentPartImageArgs::default()
                                    .image_url(screen_message)
                                    .build()
                                    .unwrap(),
                            )
                        });

                        let state = ChatCompletionRequestUserMessageContentPart::Text(
                            ChatCompletionRequestMessageContentPartTextArgs::default()
                                .text(state_message)
                                .build()
                                .unwrap(),
                        );

                        let analysis = ChatCompletionRequestUserMessageContentPart::Text(
                            ChatCompletionRequestMessageContentPartTextArgs::default()
                                .text(analysis_message)
                                .build()
                                .unwrap(),
                        );

                        let mut message_batch = vec![];

                        message_batch.extend(images);
                        message_batch.push(state);
                        message_batch.push(analysis);

                        message_batch
                    });

            complete_chat_messages.push(ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(messages.flatten().collect::<Vec<_>>())
                    .build()
                    .unwrap(),
            ));

            complete_chat_messages.push(ChatCompletionRequestMessage::Assistant(
                ChatCompletionRequestAssistantMessageArgs::default()
                    .content(message.actions_message.clone())
                    .build()
                    .unwrap(),
            ));
        });

        // let actions_message = "";
        let current_state_message = state_context.clone();
        let current_screens_message = base64_images;

        let current_pair_frame = (current_state_message, current_screens_message);

        let current_pairs = vec![current_pair_frame];

        let current_messages = current_pairs
            .into_iter()
            .map(|(state_message, screens_message)| {
                // let mut message = ChatCompletionRequestMessageContentPartText::default();
                // message.text = state_message.to_string();

                let images = screens_message.into_iter().map(|screen_message| {
                    ChatCompletionRequestUserMessageContentPart::ImageUrl(
                        ChatCompletionRequestMessageContentPartImageArgs::default()
                            .image_url(screen_message)
                            .build()
                            .unwrap(),
                    )
                });

                let state = ChatCompletionRequestUserMessageContentPart::Text(
                    ChatCompletionRequestMessageContentPartTextArgs::default()
                        .text(state_message)
                        .build()
                        .unwrap(),
                );

                // let analysis = ChatCompletionRequestUserMessageContentPart::Text(
                //     ChatCompletionRequestMessageContentPartTextArgs::default()
                //         .text(analysis_message)
                //         .build()
                //         .unwrap(),
                // );

                let mut message_batch = vec![];

                message_batch.extend(images);
                message_batch.push(state);
                // message_batch.push(analysis);

                message_batch
            });

        complete_chat_messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(current_messages.flatten().collect::<Vec<_>>())
                .build()
                .unwrap(),
        ));

        complete_chat_messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(
                    format!("
        GENERATE THE ANALYSIS JSON FOR THE CURRENT STATE.
         CURRENT SCREEN INFORMATION:
         - Screen dimensions: {width}x{height} pixels
         - Coordinate system: (0,0) is at the top-left corner
         - High DPI display: Consider scaling factors when calculating coordinates

         Analyze the CURRENT screenshot and provide a STRICT JSON response. Your response must be a valid JSON object with EXACTLY these fields:

         {{
             \"context\": string,           // Current application/window context
             \"ui_elements\": [            // Array of visible UI elements
                 {{
                     \"type\": string,     // Element type (e.g., \"button\", \"input\", \"menu\")
                     \"coords\": [         // [x1, y1, x2, y2] coordinates
                         number,           // Left edge
                         number,           // Top edge
                         number,           // Right edge
                         number            // Bottom edge
                     ]
                 }}
             ],
             \"state\": {{
                 \"focused_element\": string | null,  // Currently focused element type
                 \"selected_text\": string | null,    // Any selected text
                 \"active_window\": string,           // Active window/application
                 \"window_title\": string,            // Current window title
                 \"window_class\": string,            // Window class/type
                 \"target_window\": string | null     // Window that needs to be focused for the task
             }},
             \"challenges\": [             // Array of potential issues
                 string                    // Each challenge as a string
             ]
         }}

         IMPORTANT:
         1. Response must be ONLY the JSON object, no additional text
         2. All coordinates must be within screen bounds
         3. All fields are required
         4. Use null for empty values
         5. Do not include any explanations or comments in the JSON
         6. Always include window_title and window_class for proper window management
         7. Set target_window to the window that needs to be focused for the task (e.g., \"Chrome\" for web tasks)
         8. ONLY analyze the CURRENT screenshot, not the historical ones",
                        width = screen_width,
                        height = screen_height)
                )
                .build()
                .unwrap(),
        ));

        // Stage 1: Analysis
        let analysis_request = CreateChatCompletionRequestArgs::default()
            .model(&model_name)
            .max_tokens(max_tokens)
            .messages(complete_chat_messages.clone())
            .build()
            .unwrap();

        let analysis_response = client.chat().create(analysis_request).await.unwrap();
        let mut analysis_json = String::new();
        for choice in analysis_response.choices {
            analysis_json = choice.message.content.unwrap_or_default();
            println!("Analysis Response: {}", analysis_json);
        }

        // Clean up and validate the analysis JSON
        let clean_analysis = analysis_json
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Save analysis JSON
        let analysis_file_name = format!("{}/analysis.json", iteration_dir);
        fs::write(&analysis_file_name, clean_analysis).unwrap();

        // Validate analysis JSON structure
        let parsed_analysis = match serde_json::from_str::<serde_json::Value>(clean_analysis) {
            Ok(json) => json,
            Err(e) => {
                println!("Error: Invalid analysis JSON format: {}", e);
                println!("Attempting to fix truncated JSON...");

                // Try to fix the most common case of truncated JSON by adding missing brackets/braces
                let mut fixed_json = clean_analysis.to_string();

                // Count opening and closing braces/brackets to detect imbalance
                let open_braces = fixed_json.matches('{').count();
                let close_braces = fixed_json.matches('}').count();
                let open_brackets = fixed_json.matches('[').count();
                let close_brackets = fixed_json.matches(']').count();

                // Add missing closing braces/brackets
                for _ in 0..(open_braces - close_braces) {
                    fixed_json.push('}');
                }

                for _ in 0..(open_brackets - close_brackets) {
                    fixed_json.push(']');
                }

                // Try parsing again with fixed JSON
                match serde_json::from_str::<serde_json::Value>(&fixed_json) {
                    Ok(json) => {
                        println!("Successfully fixed truncated JSON");
                        // Update the file with fixed JSON
                        fs::write(&analysis_file_name, &fixed_json).unwrap();
                        json
                    }
                    Err(e) => {
                        println!("Could not fix JSON, continuing with partial data: {}", e);
                        // Create a minimal valid JSON with error message
                        serde_json::json!({
                            "context": "error",
                            "ui_elements": [],
                            "state": {
                                "focused_element": null,
                                "selected_text": null,
                                "active_window": "unknown",
                                "window_title": "unknown",
                                "window_class": "unknown",
                                "target_window": null
                            },
                            "challenges": ["JSON parsing error, possible truncation"]
                        })
                    }
                }
            }
        };

        let current_analysis_message = serde_json::to_string_pretty(&parsed_analysis)
            .unwrap_or_else(|_| clean_analysis.to_string());

        complete_chat_messages.push(ChatCompletionRequestMessage::Assistant(
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(format!("ANALYSIS: {}", current_analysis_message))
                .build()
                .unwrap(),
        ));

        complete_chat_messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(format!("
                Based on this context analysis and the instruction '{}', plan a sequence of actions. Your response must be a STRICT JSON array of actions.
                Remember use correct key combinations for the current operating system.
                Available Actions (use ONLY these exact formats):
                1. Window Focus:
                   {{ \"action\": \"window_focus\", \"title\": string, \"class\": string, \"method\": \"alt_tab\" | \"super_tab\" }}
                
                2. Mouse Movement:
                   {{ \"action\": \"mouse_move\", \"x\": number, \"y\": number }}
                
                3. Mouse Click:
                   {{ \"action\": \"mouse_click\", \"button\": \"left\" | \"right\" | \"middle\" }}
                
                4. Key Press:
                   {{ \"action\": \"key_press\", \"key\": \"return\" | \"tab\" | \"escape\" }}
                
                5. Key Combination:
                   {{ \"action\": \"key_combination\", \"keys\": [\"control\" | \"alt\" | \"shift\" | \"meta\" | \"cmd\", string] }}
                
                6. Text Input:
                   {{ \"action\": \"text_input\", \"text\": string }}
                
                7. Wait:
                   {{ \"action\": \"wait\", \"ms\": number }}
                   
                8. Task Done:
                   {{ \"action\": \"task_done\", \"reason\": string }}
                
                Guidelines:
                1. Response must be ONLY the JSON array, no additional text
                2. Each action must follow the exact format shown above
                3. Wait times should be between 100-1000ms
                4. Mouse coordinates must be within screen bounds
                5. Key combinations must include at least one modifier key
                6. Do not include any explanations or comments in the JSON
                7. ALWAYS start with window_focus action if the target window is not already active
                8. Add a wait after window_focus to ensure the window is ready
                9. Use super_tab for window switching if alt_tab doesn't work
                10. Verify window focus before proceeding with actions
                
                Example valid response:
                [
                    {{ \"action\": \"window_focus\", \"title\": \"Google Chrome\", \"class\": \"chrome\", \"method\": \"super_tab\" }},
                    {{ \"action\": \"wait\", \"ms\": 500 }},
                    {{ \"action\": \"key_combination\", \"keys\": [\"control\", \"t\"] }},
                    {{ \"action\": \"wait\", \"ms\": 500 }},
                    {{ \"action\": \"text_input\", \"text\": \"google.com\" }},
                    {{ \"action\": \"wait\", \"ms\": 200 }},
                    {{ \"action\": \"key_press\", \"key\": \"return\" }}
                ]", instruction))
                .build()
                .unwrap(),
        ));

        // Stage 2: Action Planning
        let action_request = CreateChatCompletionRequestArgs::default()
            .model(&model_name)
            .max_tokens(max_tokens)
            .messages(complete_chat_messages)
            .build()
            .unwrap();

        let action_response = client.chat().create(action_request).await.unwrap();
        let mut action_json = String::new();

        for choice in action_response.choices {
            action_json = choice.message.content.unwrap_or_default();
            println!("Action Plan: {}", action_json);
        }

        // Clean up and validate the action JSON
        let clean_action = action_json
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Save action JSON
        let action_file_name = format!("{}/actions.json", iteration_dir);
        fs::write(&action_file_name, clean_action).unwrap();

        // Generate self-instruction for next iteration if task is not complete
        if task_state.status != "completed" {
            let new_instruction = generate_self_instruction(
                &client,
                &model_name,
                // &iterations_history,
                &instruction,
                &task_state,
                max_tokens,
            )
            .await;
            println!("Generated new instruction: {}", new_instruction);
            *current_instruction.lock().unwrap() = new_instruction;
        }

        // Validate action JSON structure
        if let Err(e) = serde_json::from_str::<Vec<serde_json::Value>>(clean_action) {
            println!("Error: Invalid action JSON format: {}", e);
            continue;
        }

        // Stage 3: Execution
        if let Ok(actions) = serde_json::from_str::<Vec<serde_json::Value>>(clean_action) {
            for action in actions {
                if !*should_continue.lock().unwrap() {
                    break;
                }

                // Execute the action and verify it
                let action_result = match action["action"].as_str() {
                    Some("window_focus") => {
                        let mut result;
                        if let (Some(title), Some(class), Some(method)) = (
                            action["title"].as_str(),
                            action["class"].as_str(),
                            action["method"].as_str(),
                        ) {
                            println!("Focusing window: {} ({}) using {}", title, class, method);

                            // Use our custom keyboard module
                            let mut keyboard = CustomKeyboard::new(&mut enigo);
                            let key_result = match method {
                                "alt_tab" => keyboard.alt_tab(),
                                "super_tab" => keyboard.cmd_tab(),
                                _ => {
                                    println!("Unknown window focus method: {}", method);
                                    Err(format!("Unknown window focus method: {}", method))
                                }
                            };

                            // Check if the operation was successful
                            if let Err(err) = key_result {
                                println!("Error focusing window: {}", err);
                                result = ActionResult::new("window_focus").with_error(&err);
                            } else {
                                // Wait a bit for the window to focus
                                sleep(Duration::from_millis(500));

                                // Verify the action
                                result = retry_action(
                                    &action,
                                    &parsed_analysis,
                                    &mut task_state,
                                    &mut enigo,
                                );
                            }
                        } else {
                            println!("Error: Missing parameters for window_focus action");
                            result =
                                ActionResult::new("window_focus").with_error("Missing parameters");
                        }
                        result
                    }
                    Some("mouse_move") => {
                        let result;
                        if let (Some(x), Some(y)) = (action["x"].as_i64(), action["y"].as_i64()) {
                            println!("Moving mouse to ({}, {})", x, y);
                            let mut mouse = CustomMouse::new(&mut enigo);
                            match mouse.move_to(x as i32, y as i32) {
                                Ok(_) => {
                                    // Verify the action
                                    result = retry_action(
                                        &action,
                                        &parsed_analysis,
                                        &mut task_state,
                                        &mut enigo,
                                    );
                                }
                                Err(err) => {
                                    println!("Error moving mouse: {}", err);
                                    result = ActionResult::new("mouse_move").with_error(&err);
                                }
                            }
                        } else {
                            println!("Error: Missing coordinates for mouse_move action");
                            result =
                                ActionResult::new("mouse_move").with_error("Missing coordinates");
                        }
                        result
                    }
                    Some("mouse_click") => {
                        let result;
                        if let Some(button) = action["button"].as_str() {
                            println!("Clicking {} mouse button", button);
                            let mut mouse = CustomMouse::new(&mut enigo);
                            match mouse.click(button) {
                                Ok(_) => {
                                    // Verify the action
                                    result = retry_action(
                                        &action,
                                        &parsed_analysis,
                                        &mut task_state,
                                        &mut enigo,
                                    );
                                }
                                Err(err) => {
                                    println!("Error clicking mouse: {}", err);
                                    result = ActionResult::new("mouse_click").with_error(&err);
                                }
                            }
                        } else {
                            println!("Error: Missing button for mouse_click action");
                            result = ActionResult::new("mouse_click").with_error("Missing button");
                        }
                        result
                    }
                    Some("key_press") => {
                        let result;
                        if let Some(key) = action["key"].as_str() {
                            println!("Pressing key: {}", key);

                            // Use our custom keyboard module
                            let mut keyboard = CustomKeyboard::new(&mut enigo);
                            match keyboard.press_key(key) {
                                Ok(_) => {
                                    // Verify the action
                                    result = retry_action(
                                        &action,
                                        &parsed_analysis,
                                        &mut task_state,
                                        &mut enigo,
                                    );
                                }
                                Err(err) => {
                                    println!("Error pressing key: {}", err);
                                    result = ActionResult::new("key_press").with_error(&err);
                                }
                            }
                        } else {
                            println!("Error: Missing key for key_press action");
                            result = ActionResult::new("key_press").with_error("Missing key");
                        }
                        result
                    }
                    Some("key_combination") => {
                        let result;
                        if let Some(keys) = action["keys"].as_array() {
                            let key_names: Vec<String> = keys
                                .iter()
                                .filter_map(|k| k.as_str())
                                .map(|s| s.to_lowercase())
                                .collect();
                            println!("Pressing key combination_: {:?}", key_names);

                            // Use our custom keyboard module
                            let mut keyboard = CustomKeyboard::new(&mut enigo);
                            match keyboard.press_key_combination(&key_names) {
                                Ok(_) => {
                                    // Verify the action
                                    result = retry_action(
                                        &action,
                                        &parsed_analysis,
                                        &mut task_state,
                                        &mut enigo,
                                    );
                                }
                                Err(err) => {
                                    println!("Error pressing key combination: {}", err);
                                    result = ActionResult::new("key_combination").with_error(&err);
                                }
                            }
                        } else {
                            println!("Error: Missing keys for key_combination action");
                            result =
                                ActionResult::new("key_combination").with_error("Missing keys");
                        }
                        result
                    }
                    Some("text_input") => {
                        let result;

                        if let Some(text) = action["text"].as_str() {
                            println!("Typing text: {}", text);

                            // Use our custom keyboard module
                            let mut keyboard = CustomKeyboard::new(&mut enigo);
                            match keyboard.type_text(text) {
                                Ok(_) => {
                                    // Verify the action
                                    result = retry_action(
                                        &action,
                                        &parsed_analysis,
                                        &mut task_state,
                                        &mut enigo,
                                    );
                                }
                                Err(err) => {
                                    println!("Error typing text: {}", err);
                                    result = ActionResult::new("text_input").with_error(&err);
                                }
                            }
                        } else {
                            println!("Error: Missing text for text_input action");
                            result = ActionResult::new("text_input").with_error("Missing text");
                        }
                        result
                    }
                    Some("wait") => {
                        let result;
                        if let Some(ms) = action["ms"].as_i64() {
                            println!("Waiting for {}ms", ms);
                            sleep(Duration::from_millis(ms as u64));

                            // Wait actions always succeed
                            result = ActionResult::new("wait").success();
                        } else {
                            println!("Error: Missing ms for wait action");
                            result = ActionResult::new("wait").with_error("Missing ms");
                        }
                        result
                    }
                    Some("task_done") => {
                        let result;
                        if let Some(reason) = action["reason"].as_str() {
                            println!("Task done. Reason: {}", reason);
                            task_state.set_task_done();
                            save_task_state(&iteration_dir, &task_state);
                            *is_idle.lock().unwrap() = true;

                            // Task done actions always succeed
                            result = ActionResult::new("task_done").success();
                        } else {
                            println!("Error: Missing reason for task_done action");
                            result = ActionResult::new("task_done").with_error("Missing reason");
                        }
                        result
                    }
                    _ => {
                        println!("Unknown action: {:?}", action["action"]);
                        ActionResult::new("unknown").with_error("Unknown action type")
                    }
                };

                // Check if the action was successful
                if !action_result.success {
                    println!("Action failed: {:?}", action_result.error_message);

                    // If we've retried too many times, pause the task
                    if action_result.retry_count >= 3 {
                        // Too many retries for action. Pausing task.
                        println!(
                            "Too many retries for action: {}. Pausing task.",
                            action_result.action_type
                        );
                        task_state.status = "paused".to_string();
                        save_task_state(&iteration_dir, &task_state);
                        sleep(Duration::from_secs(5));
                        break;
                    }
                }
            }
        }

        println!("action time: {:?}", start.elapsed());

        // Add a small delay between iterations to prevent too rapid execution
        sleep(Duration::from_millis(500));
    }

    // Wait for the input thread to finish
    input_handle.join().unwrap();

    Ok(())
}
