use std::error::Error;

use base64::Engine;
use image::{GenericImageView, ImageFormat, ImageReader, imageops::FilterType};
use xcap::Monitor;

pub struct Screens {
    monitors: Vec<Monitor>,
}

impl Screens {
    pub fn new() -> Self {
        let monitors = Monitor::all().unwrap();

        Self { monitors }
    }

    pub fn get_monitors(&self) -> &Vec<Monitor> {
        &self.monitors
    }

    pub fn report(&self) {
        for (i, monitor) in self.monitors.iter().enumerate() {
            println!("Monitor {}: {}", i, monitor.name().unwrap());
        }
    }

    fn capture_and_save_all(&self, path: &str) -> Result<Vec<String>, Box<dyn Error>> {
        let mut file_names = Vec::new();

        for (i, monitor) in self.monitors.iter().enumerate() {
            let image = monitor.capture_image().unwrap();

            // let image_file_name = format!("{}/screenshot.png", iteration_dir);
            // image.save(&image_file_name).unwrap();
            let img_path = format!("{}/monitor_{}.png", path, i);
            image.save(img_path.clone())?;

            file_names.push(img_path);
        }

        Ok(file_names)
    }

    pub fn capture_and_save_all_with_base64(
        &self,
        path: &str,
        resize_factor: u32,
    ) -> Result<Vec<String>, Box<dyn Error>> {
        let file_names = self.capture_and_save_all(path)?;

        let mut base64_images = Vec::new();

        for file_name in file_names {
            // let start = Instant::now();

            let img = ImageReader::open(&file_name).unwrap();

            let img = img.decode().unwrap();

            let (w, h) = img.dimensions();
            let img = img.resize(w / resize_factor, h / resize_factor, FilterType::CatmullRom);

            let resized_image_file_name = format!("{}/screenshot_resized.png", path);
            img.save(&resized_image_file_name).unwrap();

            // Create a buffer to store the image data
            let mut buf = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut buf);
            img.write_to(&mut cursor, ImageFormat::Png).unwrap();

            // Encode the image data to base64
            let res_base64 = base64::engine::general_purpose::STANDARD.encode(&buf);

            // println!("encode time: {:?}", start.elapsed());
            base64_images.push(res_base64);
        }

        Ok(base64_images)
    }
}

impl Default for Screens {
    fn default() -> Self {
        Self::new()
    }
}
