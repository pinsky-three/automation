use crate::models::{ActionResult, TaskState};
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse};
use std::thread::sleep;
use std::time::Duration;

pub fn execute_action(
    action: &serde_json::Value,
    task_state: &mut TaskState,
    enigo: &mut Enigo,
) -> ActionResult {
    let mut result = ActionResult::new(action["action"].as_str().unwrap_or("unknown"));

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

                // Wait a bit for the window to focus
                sleep(Duration::from_millis(500));
                result.mark_success();
            } else {
                result.mark_error("Missing parameters for window_focus action");
            }
        }
        Some("mouse_move") => {
            if let (Some(x), Some(y)) = (action["x"].as_i64(), action["y"].as_i64()) {
                println!("Moving mouse to ({}, {})", x, y);
                enigo
                    .move_mouse(x as i32, y as i32, Coordinate::Abs)
                    .unwrap();
                result.mark_success();
            } else {
                result.mark_error("Missing coordinates for mouse_move action");
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
                result.mark_success();
            } else {
                result.mark_error("Missing button for mouse_click action");
            }
        }
        Some("key_press") => {
            if let Some(key) = action["key"].as_str() {
                println!("Pressing key: {}", key);
                match key.to_lowercase().as_str() {
                    "return" | "enter" => enigo.key(Key::Return, Direction::Click).unwrap(),
                    "tab" => enigo.key(Key::Tab, Direction::Click).unwrap(),
                    "escape" => enigo.key(Key::Escape, Direction::Click).unwrap(),
                    _ => println!("Unknown key: {}", key),
                }
                result.mark_success();
            } else {
                result.mark_error("Missing key for key_press action");
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
                result.mark_success();
            } else {
                result.mark_error("Missing keys for key_combination action");
            }
        }
        Some("text_input") => {
            if let Some(text) = action["text"].as_str() {
                println!("Typing text: {}", text);
                enigo.text(text).unwrap();
                result.mark_success();
            } else {
                result.mark_error("Missing text for text_input action");
            }
        }
        Some("wait") => {
            if let Some(ms) = action["ms"].as_i64() {
                println!("Waiting for {}ms", ms);
                sleep(Duration::from_millis(ms as u64));
                result.mark_success();
            } else {
                result.mark_error("Missing ms for wait action");
            }
        }
        Some("task_done") => {
            if let Some(reason) = action["reason"].as_str() {
                println!("Task done. Reason: {}", reason);
                task_state.set_task_done();
                result.mark_success();
            } else {
                result.mark_error("Missing reason for task_done action");
            }
        }
        _ => {
            println!("Unknown action: {:?}", action["action"]);
            result.mark_error("Unknown action type");
        }
    }

    // Add the result to the task state
    task_state.add_action_result(result.clone());
    result
}
