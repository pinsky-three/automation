use async_openai::Client;
use async_openai::config::{Config, OpenAIConfig};
use async_openai::types::{
    ChatCompletionRequestMessageContentPartImageArgs,
    ChatCompletionRequestMessageContentPartTextArgs, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs, ImageDetail, ImageUrlArgs,
};
use base64::Engine;
use enigo::{Button, Enigo, Keyboard, Mouse, Settings};
use fs_extra::dir;
use image::imageops::FilterType;
use image::{GenericImageView, ImageFormat, ImageReader};
use std::time::Instant;
use std::{thread::sleep, time::Duration};
use xcap::Monitor;

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    let client = Client::with_config(
        OpenAIConfig::new()
            .with_api_base("https://openrouter.ai/api/v1")
            .with_api_key(""),
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

    let img = ImageReader::open(image_file_name.clone()).unwrap();

    let img = img.decode().unwrap();

    let (w, h) = img.dimensions();
    let img = img.resize(w / 3, h / 3, FilterType::CatmullRom);

    img.save("target/monitors/monitor-1-resized.png").unwrap();

    // Create a buffer to store the image data
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, ImageFormat::Png).unwrap();

    // Encode the image data to base64
    let res_base64 = base64::encode(&buf);

    // ---

    let request = CreateChatCompletionRequestArgs::default()
        .model("openai/gpt-4o-mini")
        .max_tokens(300_u32)
        .messages([ChatCompletionRequestUserMessageArgs::default()
            .content(vec![
                ChatCompletionRequestMessageContentPartTextArgs::default()
                    .text("What is this image?")
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

    for choice in response.choices {
        println!(
            "{}: Role: {}  Content: {:?}",
            choice.index,
            choice.message.role,
            choice.message.content.unwrap_or_default()
        );
    }

    // ---

    let mut enigo = Enigo::new(&Settings::default()).unwrap();

    // enigo.delay();

    // enigo.move_mouse(500, 200, enigo::Coordinate::Abs).unwrap();
    // enigo.button(Button::Left, enigo::Direction::Click).unwrap();
    enigo
        .key(enigo::Key::Meta, enigo::Direction::Click)
        .unwrap();

    let (w, h) = enigo.main_display().unwrap();

    println!("w: {}, h: {}", w, h);

    sleep(Duration::from_millis(100));

    enigo.text("Hello World!").unwrap();
}

fn normalized(filename: String) -> String {
    filename.replace(['|', '\\', ':', '/'], "")
}
