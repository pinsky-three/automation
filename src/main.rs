use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestMessageContentPartImageArgs,
    ChatCompletionRequestMessageContentPartTextArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs, ImageDetail, ImageUrlArgs,
};
use base64::Engine;
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use fs_extra::dir;
use image::imageops::FilterType;
use image::{GenericImageView, ImageFormat, ImageReader};
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use std::{thread::sleep, time::Duration};
use xcap::Monitor;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let api_base =
        std::env::var("API_BASE").unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());
    let model_name =
        std::env::var("MODEL_NAME").unwrap_or_else(|_| "google/gemini-flash-1.5-8b".to_string());
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

        dir::create_all("target/monitors", true).unwrap();

        let monitor = monitors.first().unwrap();
        let image = monitor.capture_image().unwrap();

        let image_file_name = "target/monitors/monitor-1.png";

        image.save(&image_file_name).unwrap();

        println!("capture time: {:?}", start.elapsed());

        // ---

        let start = Instant::now();

        let img = ImageReader::open(image_file_name).unwrap();

        let img = img.decode().unwrap();

        let (w, h) = img.dimensions();
        let img = img.resize(w / 4, h / 4, FilterType::CatmullRom);

        img.save("target/monitors/monitor-1-resized.png").unwrap();

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

        // Stage 1: Analysis
        let analysis_request = CreateChatCompletionRequestArgs::default()
            .model(&model_name)
            .max_tokens(300_u32)
            .messages([ChatCompletionRequestUserMessageArgs::default()
                .content(vec![
                    ChatCompletionRequestMessageContentPartTextArgs::default()
                        .text(format!("Analyze this screenshot and provide a detailed context analysis. Consider:
1. Current context (e.g., browser window, text editor, desktop)
2. Visible UI elements (buttons, text fields, menus) with their coordinates
3. Current state (e.g., text selected, window focused)
4. Any potential obstacles or challenges

Screen Information:
- Screen dimensions: {}x{} pixels
- Coordinate system: (0,0) is at the top-left corner
- High DPI display: Consider scaling factors when calculating coordinates

Respond with a JSON object containing your analysis. Example format:
{{
    \"context\": \"Chrome browser window with google.com tab\",
    \"ui_elements\": [
        {{ \"type\": \"address_bar\", \"coords\": [100, 50, 500, 80] }},
        {{ \"type\": \"new_tab_button\", \"coords\": [800, 10, 830, 40] }}
    ],
    \"state\": {{
        \"focused_element\": \"address_bar\",
        \"selected_text\": null,
        \"active_window\": \"chrome\"
    }},
    \"challenges\": [
        \"Need to ensure Chrome window is focused before actions\"
    ]
}}", screen_width, screen_height))
                        .build()
                        .unwrap()
                        .into(),
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
                ])
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

        // Clean up the analysis JSON
        let clean_analysis = analysis_json
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Stage 2: Action Planning
        let action_request = CreateChatCompletionRequestArgs::default()
            .model(&model_name)
            .max_tokens(300_u32)
            .messages([ChatCompletionRequestUserMessageArgs::default()
                .content(vec![
                    ChatCompletionRequestMessageContentPartTextArgs::default()
                        .text(format!("Based on this context analysis and the instruction '{}', plan a sequence of actions to accomplish the task.

Context Analysis:
{}

Available Basic Actions (use ONLY these):
1. mouse_move(x, y): Move mouse to absolute coordinates
2. mouse_click(button): Click mouse button (left, right, middle)
3. key_press(key): Press a single key (return, tab, escape)
4. key_combination(keys): Press multiple keys simultaneously (e.g., ['control', 't'] for Ctrl+T)
5. text_input(text): Type text
6. wait(ms): Wait for milliseconds

Guidelines for Reliable Automation:
1. Always add small waits (100-500ms) between actions to ensure they complete
2. For mouse movements, verify the target is visible in the screenshot
3. For text input, ensure the target field is focused
4. For key combinations, use the key_combination action instead of separate key_press actions
5. Break complex tasks into small, reliable steps
6. Consider the current context and state when planning actions

Respond with a JSON array of actions to accomplish the instruction. Example:
[
    {{\"action\": \"key_combination\", \"keys\": [\"control\", \"t\"]}},
    {{\"action\": \"wait\", \"ms\": 500}},
    {{\"action\": \"text_input\", \"text\": \"google.com\"}},
    {{\"action\": \"wait\", \"ms\": 200}},
    {{\"action\": \"key_press\", \"key\": \"return\"}}
]", instruction, clean_analysis))
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

        // Clean up the action JSON
        let clean_action = action_json
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Parse and execute the actions
        if let Ok(actions) = serde_json::from_str::<Vec<serde_json::Value>>(clean_action) {
            for action in actions {
                if !*should_continue.lock().unwrap() {
                    break;
                }
                match action["action"].as_str() {
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
