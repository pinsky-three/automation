use enigo::{Button, Enigo, Keyboard, Mouse, Settings};
use fs_extra::dir;
use std::time::Instant;
use std::{thread::sleep, time::Duration};
use xcap::Monitor;

fn main() {
    println!("Hello, world!");

    let start = Instant::now();
    let monitors = Monitor::all().unwrap();

    dir::create_all("target/monitors", true).unwrap();

    for monitor in monitors {
        let image = monitor.capture_image().unwrap();

        image
            .save(format!(
                "target/monitors/monitor-{}.png",
                normalized(monitor.name().unwrap())
            ))
            .unwrap();
    }

    println!("capture time: {:?}", start.elapsed());

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
