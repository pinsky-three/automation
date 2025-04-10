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

    let api_key = std::env::var("API_KEY").unwrap();

    let client = Client::with_config(
        OpenAIConfig::new()
            .with_api_base("https://openrouter.ai/api/v1")
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
        let request = CreateChatCompletionRequestArgs::default()
            .model("openai/gpt-4o-mini")
            .max_tokens(300_u32)
            .messages([ChatCompletionRequestUserMessageArgs::default()
                .content(vec![
                    ChatCompletionRequestMessageContentPartTextArgs::default()
                        .text(format!("Based on this screenshot and the instruction '{}', what should be the next sequence of actions? The screen dimensions are {}x{} pixels. Respond with a JSON array of actions. Each action should be an object with 'action' and parameters. Available actions are: mouse_move(x, y), mouse_click(button), key_press(key), text_input(text), wait(ms), stop. Example: [{{\"action\": \"mouse_move\", \"x\": 100, \"y\": 200}}, {{\"action\": \"mouse_click\", \"button\": \"left\"}}, {{\"action\": \"wait\", \"ms\": 1000}}, {{\"action\": \"stop\"}}]", instruction, screen_width, screen_height))
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

        let response = client.chat().create(request).await.unwrap();

        let mut action_json = String::new();
        for choice in response.choices {
            action_json = choice.message.content.unwrap_or_default();
            println!("AI Response: {}", action_json);
        }

        // Clean up the JSON string by removing markdown formatting
        let clean_json = action_json
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Parse the JSON response and execute the actions
        if let Ok(actions) = serde_json::from_str::<Vec<serde_json::Value>>(clean_json) {
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
                            match key {
                                "enter" => enigo.key(Key::Return, Direction::Click).unwrap(),
                                "tab" => enigo.key(Key::Tab, Direction::Click).unwrap(),
                                "escape" => enigo.key(Key::Escape, Direction::Click).unwrap(),
                                _ => println!("Unknown key: {}", key),
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
                    Some("stop") => {
                        println!("AI suggested stopping, but continuing with next iteration...");
                        // Don't stop the loop, just continue to next iteration
                        break;
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
