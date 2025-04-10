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
use serde_json;
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

    // let image_url = "https://upload.wikimedia.org/wikipedia/commons/thumb/d/dd/Gfp-wisconsin-madison-the-nature-boardwalk.jpg/2560px-Gfp-wisconsin-madison-the-nature-boardwalk.jpg";

    let start = Instant::now();
    let monitors = Monitor::all().unwrap();

    dir::create_all("target/monitors", true).unwrap();

    let monitor = monitors.first().unwrap();
    let image = monitor.capture_image().unwrap();

    let image_file_name = format!(
        "target/monitors/monitor-{}.png",
        normalized(monitor.name().unwrap())
    );

    image.save(&image_file_name).unwrap();

    println!("capture time: {:?}", start.elapsed());

    // ---

    let start = Instant::now();

    let img = ImageReader::open(image_file_name.clone()).unwrap();

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

    let mut enigo = Enigo::new(&Settings::default()).unwrap();

    let request = CreateChatCompletionRequestArgs::default()
        .model("openai/gpt-4o-mini")
        .max_tokens(300_u32)
        .messages([ChatCompletionRequestUserMessageArgs::default()
            .content(vec![
                ChatCompletionRequestMessageContentPartTextArgs::default()
                    .text("Based on this screenshot, what should be the next action? Respond with a JSON object containing the action type and parameters. Available actions are: mouse_move(x, y), mouse_click(button), key_press(key), text_input(text). Example: {\"action\": \"mouse_move\", \"x\": 100, \"y\": 200}")
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

    // println!("{}", serde_json::to_string(&request).unwrap());

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

    // Parse the JSON response and execute the action
    if let Ok(action) = serde_json::from_str::<serde_json::Value>(&clean_json) {
        match action["action"].as_str() {
            Some("mouse_move") => {
                if let (Some(x), Some(y)) = (action["x"].as_i64(), action["y"].as_i64()) {
                    enigo
                        .move_mouse(x as i32, y as i32, Coordinate::Abs)
                        .unwrap();
                }
            }
            Some("mouse_click") => {
                if let Some(button) = action["button"].as_str() {
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
                    enigo.text(text).unwrap();
                }
            }
            _ => println!("Unknown action: {:?}", action["action"]),
        }
    }

    println!("action time: {:?}", start.elapsed());
}

fn normalized(filename: String) -> String {
    filename.replace(['|', '\\', ':', '/'], "")
}
