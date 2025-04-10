use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestMessageContentPartImageArgs,
    ChatCompletionRequestMessageContentPartTextArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs, ImageDetail, ImageUrlArgs,
};
use base64::Engine;
use chrono::Local;
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use fs_extra::dir;
use image::imageops::FilterType;
use image::{GenericImageView, ImageFormat, ImageReader};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{thread::sleep, time::Duration};
use xcap::Monitor;

// Function to get the last N iterations
fn get_last_n_iterations(n: usize) -> Vec<(String, String, String)> {
    let iterations_dir = Path::new("target/iterations");
    if !iterations_dir.exists() {
        return Vec::new();
    }

    let mut iterations: Vec<(String, String, String)> = Vec::new();

    // Get all iteration directories and sort them by name (timestamp) in descending order
    let mut dirs: Vec<_> = fs::read_dir(iterations_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect();
    dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    // Take the last N iterations
    for entry in dirs.iter().take(n) {
        let dir_path = entry.path();
        let metadata_path = dir_path.join("metadata.json");
        let analysis_path = dir_path.join("analysis.json");
        let actions_path = dir_path.join("actions.json");

        if metadata_path.exists() && analysis_path.exists() && actions_path.exists() {
            if let (Ok(metadata), Ok(analysis), Ok(actions)) = (
                fs::read_to_string(&metadata_path),
                fs::read_to_string(&analysis_path),
                fs::read_to_string(&actions_path),
            ) {
                iterations.push((metadata, analysis, actions));
            }
        }
    }

    iterations
}

// Function to get screenshot from iteration directory
fn get_screenshot_from_iteration(dir_path: &Path) -> Option<String> {
    let screenshot_path = dir_path.join("screenshot_resized.png");
    if screenshot_path.exists() {
        if let Ok(img) = ImageReader::open(&screenshot_path) {
            if let Ok(img) = img.decode() {
                let (w, h) = img.dimensions();
                let img = img.resize(w / 3, h / 3, FilterType::CatmullRom);

                // Create a buffer to store the image data
                let mut buf = Vec::new();
                let mut cursor = std::io::Cursor::new(&mut buf);
                if img.write_to(&mut cursor, ImageFormat::Png).is_ok() {
                    // Encode the image data to base64
                    return Some(base64::engine::general_purpose::STANDARD.encode(&buf));
                }
            }
        }
    }
    None
}

// Function to get the last N iterations with screenshots
fn get_last_n_iterations_with_screenshots(
    n: usize,
) -> Vec<(String, String, String, Option<String>)> {
    let iterations_dir = Path::new("target/iterations");
    if !iterations_dir.exists() {
        return Vec::new();
    }

    let mut iterations: Vec<(String, String, String, Option<String>)> = Vec::new();

    // Get all iteration directories and sort them by name (timestamp) in descending order
    let mut dirs: Vec<_> = fs::read_dir(iterations_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect();
    dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    // Take the last N iterations
    for entry in dirs.iter().take(n) {
        let dir_path = entry.path();
        let metadata_path = dir_path.join("metadata.json");
        let analysis_path = dir_path.join("analysis.json");
        let actions_path = dir_path.join("actions.json");

        if metadata_path.exists() && analysis_path.exists() && actions_path.exists() {
            if let (Ok(metadata), Ok(analysis), Ok(actions)) = (
                fs::read_to_string(&metadata_path),
                fs::read_to_string(&analysis_path),
                fs::read_to_string(&actions_path),
            ) {
                let screenshot = get_screenshot_from_iteration(&dir_path);
                iterations.push((metadata, analysis, actions, screenshot));
            }
        }
    }

    iterations
}

// Function to format iterations history for the prompt
fn format_iterations_history(iterations: &[(String, String, String, Option<String>)]) -> String {
    if iterations.is_empty() {
        return String::from("No previous iterations available.");
    }

    let mut history = String::from("Previous iterations:\n\n");

    for (metadata, analysis, actions, _) in iterations {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata) {
            if let (Some(timestamp), Some(instruction), Some(status)) = (
                meta["timestamp"].as_str(),
                meta["instruction"].as_str(),
                meta["status"].as_str(),
            ) {
                history.push_str(&format!("Iteration {}:\n", timestamp));
                history.push_str(&format!("Instruction: {}\n", instruction));
                history.push_str(&format!("Status: {}\n", status));
                if let Some(feedback) = meta["feedback"].as_str() {
                    history.push_str(&format!("Feedback: {}\n", feedback));
                }
                history.push_str("Analysis:\n");
                history.push_str(&format!("{}\n", analysis));
                history.push_str("Actions:\n");
                history.push_str(&format!("{}\n\n", actions));
            }
        }
    }

    history
}

// Function to determine if a task is complete based on the analysis
fn is_task_complete(analysis: &str, instruction: &str) -> (bool, String) {
    // Parse the analysis JSON
    let analysis_json = match serde_json::from_str::<serde_json::Value>(analysis) {
        Ok(json) => json,
        Err(_) => return (false, "Failed to parse analysis".to_string()),
    };

    // Check for completion indicators in the analysis
    let context = analysis_json["context"].as_str().unwrap_or("");

    // Get challenges array, creating an empty Vec if not present
    let empty_vec = Vec::new();
    let challenges = analysis_json["challenges"].as_array().unwrap_or(&empty_vec);

    // If there are no challenges and the context matches the instruction's goal
    if challenges.is_empty() && context.contains(instruction) {
        return (true, "Task completed successfully".to_string());
    }

    // If there are challenges, provide feedback
    if !challenges.is_empty() {
        let feedback: String = challenges
            .iter()
            .filter_map(|c| c.as_str())
            .collect::<Vec<&str>>()
            .join(", ");
        return (false, format!("Challenges encountered: {}", feedback));
    }

    (false, "Task in progress".to_string())
}

// Function to generate self-instruction based on history
async fn generate_self_instruction(
    client: &Client<OpenAIConfig>,
    model_name: &str,
    history: &[(String, String, String, Option<String>)],
    current_instruction: &str,
) -> String {
    if history.is_empty() {
        return current_instruction.to_string();
    }

    let history_text = format_iterations_history(history);
    let last_iteration = &history[0];
    let (_, feedback) = is_task_complete(&last_iteration.1, current_instruction);

    let self_instruction_request = CreateChatCompletionRequestArgs::default()
        .model(model_name)
        .max_tokens(256_u32)
        .messages([ChatCompletionRequestUserMessageArgs::default()
            .content(vec![
                ChatCompletionRequestMessageContentPartTextArgs::default()
                    .text(format!(
                        "Based on the following history and feedback, generate a refined instruction to achieve the original goal: '{}'

History:
{}

Feedback from last attempt: {}

Generate a new instruction that:
1. Addresses the feedback from previous attempts
2. Maintains the original goal
3. Is clear and specific
4. Focuses on overcoming identified challenges

Response should be ONLY the new instruction, no additional text.",
                        current_instruction, history_text, feedback
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

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let api_base =
        std::env::var("API_BASE").unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());
    let model_name =
        std::env::var("MODEL_NAME").unwrap_or_else(|_| "google/gemini-2.0-flash-001".to_string());
    let max_tokens = std::env::var("MAX_TOKENS")
        .unwrap_or_else(|_| "512".to_string())
        .parse::<u32>()
        .unwrap_or(512);

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

    while *should_continue.lock().unwrap() {
        // Check if we're in idle state
        if *is_idle.lock().unwrap() {
            sleep(Duration::from_millis(100));
            continue;
        }

        let start = Instant::now();
        let monitors = Monitor::all().unwrap();

        // Create timestamp for this iteration
        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let iteration_dir = format!("target/iterations/{}", timestamp);
        fs::create_dir_all(&iteration_dir).unwrap();

        dir::create_all("target/monitors", true).unwrap();

        let monitor = monitors.first().unwrap();
        let image = monitor.capture_image().unwrap();

        let image_file_name = format!("{}/screenshot.png", iteration_dir);
        image.save(&image_file_name).unwrap();

        println!("capture time: {:?}", start.elapsed());

        // ---

        let start = Instant::now();

        let img = ImageReader::open(&image_file_name).unwrap();

        let img = img.decode().unwrap();

        let (w, h) = img.dimensions();
        let img = img.resize(w / 3, h / 3, FilterType::CatmullRom);

        let resized_image_file_name = format!("{}/screenshot_resized.png", iteration_dir);
        img.save(&resized_image_file_name).unwrap();

        // Create a buffer to store the image data
        let mut buf = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        img.write_to(&mut cursor, ImageFormat::Png).unwrap();

        // Encode the image data to base64
        let res_base64 = base64::engine::general_purpose::STANDARD.encode(&buf);

        println!("encode time: {:?}", start.elapsed());

        // ---

        let start = Instant::now();

        let instruction = current_instruction.lock().unwrap().clone();

        // Get the last 3 iterations with screenshots for context
        let iterations_history = get_last_n_iterations_with_screenshots(3);
        let history_text = format_iterations_history(&iterations_history);

        // Prepare content array with current and historical screenshots
        let mut content_parts = vec![
            ChatCompletionRequestMessageContentPartTextArgs::default()
                .text(format!("{history}

CURRENT STATE ANALYSIS:
You are analyzing the current state of the screen. Below is the history of previous attempts for context.

HISTORY OF PREVIOUS ATTEMPTS:
{history}

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
                    history = history_text,
                    width = screen_width,
                    height = screen_height))
                .build()
                .unwrap()
                .into()
        ];

        // Add current screenshot first
        content_parts.push(
            ChatCompletionRequestMessageContentPartImageArgs::default()
                .image_url(
                    ImageUrlArgs::default()
                        .url(format!("data:image/png;base64,{}", res_base64))
                        .detail(ImageDetail::High)
                        .build()
                        .unwrap(),
                )
                .build()
                .unwrap()
                .into(),
        );

        // Add historical screenshots in reverse chronological order (oldest first)
        for (_, _, _, screenshot) in iterations_history.iter().rev() {
            if let Some(base64_img) = screenshot {
                content_parts.push(
                    ChatCompletionRequestMessageContentPartImageArgs::default()
                        .image_url(
                            ImageUrlArgs::default()
                                .url(format!("data:image/png;base64,{}", base64_img))
                                .detail(ImageDetail::High)
                                .build()
                                .unwrap(),
                        )
                        .build()
                        .unwrap()
                        .into(),
                );
            }
        }

        // Stage 1: Analysis
        let analysis_request = CreateChatCompletionRequestArgs::default()
            .model(&model_name)
            .max_tokens(max_tokens)
            .messages([ChatCompletionRequestUserMessageArgs::default()
                .content(content_parts)
                .build()
                .unwrap()
                .into()])
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
        if let Err(e) = serde_json::from_str::<serde_json::Value>(clean_analysis) {
            println!("Error: Invalid analysis JSON format: {}", e);
            continue;
        }

        // Stage 2: Action Planning
        let action_request = CreateChatCompletionRequestArgs::default()
            .model(&model_name)
            .max_tokens(max_tokens)
            .messages([ChatCompletionRequestUserMessageArgs::default()
                .content(vec![
                    ChatCompletionRequestMessageContentPartTextArgs::default()
                        .text(format!("{}

Based on this context analysis and the instruction '{}', plan a sequence of actions. Your response must be a STRICT JSON array of actions.

Context Analysis:
{}

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
   {{ \"action\": \"key_combination\", \"keys\": [\"control\" | \"alt\" | \"shift\" | \"meta\", string] }}

6. Text Input:
   {{ \"action\": \"text_input\", \"text\": string }}

7. Wait:
   {{ \"action\": \"wait\", \"ms\": number }}

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
]", history_text, instruction, clean_analysis))
                        .build()
                        .unwrap()
                        .into()])
                .build()
                .unwrap()
                .into()])
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

        // Save metadata with enhanced information
        let (is_complete, feedback) = is_task_complete(&clean_analysis, &instruction);
        let status = if is_complete {
            "completed"
        } else {
            "in_progress"
        };

        let metadata = serde_json::json!({
            "timestamp": timestamp,
            "instruction": instruction,
            "model": model_name,
            "screenshot": "screenshot.png",
            "screenshot_resized": "screenshot_resized.png",
            "analysis": "analysis.json",
            "actions": "actions.json",
            "status": status,
            "feedback": feedback
        });
        let metadata_file_name = format!("{}/metadata.json", iteration_dir);
        fs::write(
            &metadata_file_name,
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        // Generate self-instruction for next iteration if task is not complete
        if !is_complete {
            let new_instruction =
                generate_self_instruction(&client, &model_name, &iterations_history, &instruction)
                    .await;
            println!("Generated new instruction: {}", new_instruction);
            *current_instruction.lock().unwrap() = new_instruction;
        }

        // Validate action JSON structure
        if let Err(e) = serde_json::from_str::<Vec<serde_json::Value>>(clean_action) {
            println!("Error: Invalid action JSON format: {}", e);
            continue;
        }

        // Parse and execute the actions
        if let Ok(actions) = serde_json::from_str::<Vec<serde_json::Value>>(clean_action) {
            for action in actions {
                if !*should_continue.lock().unwrap() {
                    break;
                }
                match action["action"].as_str() {
                    Some("window_focus") => {
                        if let (Some(title), Some(class), Some(method)) = (
                            action["title"].as_str(),
                            action["class"].as_str(),
                            action["method"].as_str(),
                        ) {
                            println!("Focusing window: {} ({}) using {}", title, class, method);
                            match method {
                                "alt_tab" => {
                                    enigo.key(Key::Alt, Direction::Press).unwrap();
                                    sleep(Duration::from_millis(100));
                                    enigo.key(Key::Tab, Direction::Click).unwrap();
                                    sleep(Duration::from_millis(100));
                                    enigo.key(Key::Alt, Direction::Release).unwrap();
                                }
                                "super_tab" => {
                                    enigo.key(Key::Meta, Direction::Press).unwrap();
                                    sleep(Duration::from_millis(100));
                                    enigo.key(Key::Tab, Direction::Click).unwrap();
                                    sleep(Duration::from_millis(100));
                                    enigo.key(Key::Meta, Direction::Release).unwrap();
                                }
                                _ => println!("Unknown window focus method: {}", method),
                            }
                        }
                    }
                    Some("mouse_move") => {
                        if let (Some(x), Some(y)) = (action["x"].as_i64(), action["y"].as_i64()) {
                            println!("Moving mouse to ({}, {})", x, y);
                            enigo
                                .move_mouse(x as i32, y as i32, Coordinate::Abs)
                                .unwrap();
                        }
                    }
                    Some("mouse_click") => {
                        if let Some(button) = action["button"].as_str() {
                            println!("Clicking {} mouse button", button);
                            match button {
                                "left" => enigo.button(Button::Left, Direction::Click).unwrap(),
                                "right" => enigo.button(Button::Right, Direction::Click).unwrap(),
                                "middle" => enigo.button(Button::Middle, Direction::Click).unwrap(),
                                _ => println!("Unknown button: {}", button),
                            }
                        }
                    }
                    Some("key_press") => {
                        if let Some(key) = action["key"].as_str() {
                            println!("Pressing key: {}", key);
                            match key.to_lowercase().as_str() {
                                "return" | "enter" => {
                                    enigo.key(Key::Return, Direction::Click).unwrap()
                                }
                                "tab" => enigo.key(Key::Tab, Direction::Click).unwrap(),
                                "escape" => enigo.key(Key::Escape, Direction::Click).unwrap(),
                                _ => println!("Unknown key: {}", key),
                            }
                        }
                    }
                    Some("key_combination") => {
                        if let Some(keys) = action["keys"].as_array() {
                            let key_names: Vec<String> = keys
                                .iter()
                                .filter_map(|k| k.as_str())
                                .map(|s| s.to_lowercase())
                                .collect();
                            println!("Pressing key combination: {:?}", key_names);

                            // Press all modifier keys first
                            for key in &key_names {
                                match key.as_str() {
                                    "control" | "ctrl" => {
                                        enigo.key(Key::Control, Direction::Press).unwrap();
                                    }
                                    "alt" => {
                                        enigo.key(Key::Alt, Direction::Press).unwrap();
                                    }
                                    "shift" => {
                                        enigo.key(Key::Shift, Direction::Press).unwrap();
                                    }
                                    "meta" | "super" | "windows" => {
                                        enigo.key(Key::Meta, Direction::Press).unwrap();
                                    }
                                    _ => {}
                                }
                            }

                            // Small delay to ensure modifier keys are registered
                            sleep(Duration::from_millis(50));

                            // Press the last key (non-modifier)
                            if let Some(last_key) = key_names.last() {
                                match last_key.as_str() {
                                    "t" => enigo.text("t").unwrap(),
                                    "w" => enigo.text("w").unwrap(),
                                    "r" => enigo.text("r").unwrap(),
                                    "l" => enigo.text("l").unwrap(),
                                    "a" => enigo.text("a").unwrap(),
                                    "c" => enigo.text("c").unwrap(),
                                    "v" => enigo.text("v").unwrap(),
                                    "x" => enigo.text("x").unwrap(),
                                    "z" => enigo.text("z").unwrap(),
                                    _ => println!("Unknown key in combination: {}", last_key),
                                }
                            }

                            // Small delay to ensure the key combination is registered
                            sleep(Duration::from_millis(50));

                            for key in &key_names {
                                match key.as_str() {
                                    "control" | "ctrl" => {
                                        enigo.key(Key::Control, Direction::Release).unwrap();
                                    }
                                    "alt" => {
                                        enigo.key(Key::Alt, Direction::Release).unwrap();
                                    }
                                    "shift" => {
                                        enigo.key(Key::Shift, Direction::Release).unwrap();
                                    }
                                    "meta" | "super" | "windows" => {
                                        enigo.key(Key::Meta, Direction::Release).unwrap();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    Some("text_input") => {
                        if let Some(text) = action["text"].as_str() {
                            println!("Typing text: {}", text);
                            enigo.text(text).unwrap();
                        }
                    }
                    Some("wait") => {
                        if let Some(ms) = action["ms"].as_i64() {
                            println!("Waiting for {}ms", ms);
                            sleep(Duration::from_millis(ms as u64));
                        }
                    }
                    _ => println!("Unknown action: {:?}", action["action"]),
                }
            }
        }

        println!("action time: {:?}", start.elapsed());

        // Add a small delay between iterations to prevent too rapid execution
        sleep(Duration::from_millis(500));
    }

    // Wait for the input thread to finish
    input_handle.join().unwrap();
}
