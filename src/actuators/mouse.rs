use enigo::{Button, Coordinate, Direction, Enigo, Mouse as EnigoMouse};
use std::thread::sleep;
use std::time::Duration;

/// Mouse provides methods for mouse control
pub struct Mouse<'a> {
    enigo: &'a mut Enigo,
}

impl<'a> Mouse<'a> {
    /// Create a new Mouse with a reference to an Enigo instance
    pub fn new(enigo: &'a mut Enigo) -> Self {
        Mouse { enigo }
    }

    /// Move the mouse pointer to absolute coordinates
    pub fn move_to(&mut self, x: i32, y: i32) -> Result<(), String> {
        EnigoMouse::move_mouse(self.enigo, x, y, Coordinate::Abs).map_err(|e| e.to_string())
    }

    /// Click a mouse button
    pub fn click(&mut self, button_name: &str) -> Result<(), String> {
        let button = match button_name.to_lowercase().as_str() {
            "left" => Button::Left,
            "right" => Button::Right,
            "middle" => Button::Middle,
            _ => return Err(format!("Unknown mouse button: {}", button_name)),
        };

        EnigoMouse::button(self.enigo, button, Direction::Click).map_err(|e| e.to_string())
    }

    /// Double click a mouse button (typically left button)
    pub fn double_click(&mut self, button_name: &str) -> Result<(), String> {
        let button = match button_name.to_lowercase().as_str() {
            "left" => Button::Left,
            "right" => Button::Right,
            "middle" => Button::Middle,
            _ => return Err(format!("Unknown mouse button: {}", button_name)),
        };

        // First click
        EnigoMouse::button(self.enigo, button, Direction::Click).map_err(|e| e.to_string())?;

        // Small delay between clicks
        sleep(Duration::from_millis(50));

        // Second click
        EnigoMouse::button(self.enigo, button, Direction::Click).map_err(|e| e.to_string())
    }

    /// Press and hold a mouse button
    pub fn press(&mut self, button_name: &str) -> Result<(), String> {
        let button = match button_name.to_lowercase().as_str() {
            "left" => Button::Left,
            "right" => Button::Right,
            "middle" => Button::Middle,
            _ => return Err(format!("Unknown mouse button: {}", button_name)),
        };

        EnigoMouse::button(self.enigo, button, Direction::Press).map_err(|e| e.to_string())
    }

    /// Release a mouse button
    pub fn release(&mut self, button_name: &str) -> Result<(), String> {
        let button = match button_name.to_lowercase().as_str() {
            "left" => Button::Left,
            "right" => Button::Right,
            "middle" => Button::Middle,
            _ => return Err(format!("Unknown mouse button: {}", button_name)),
        };

        EnigoMouse::button(self.enigo, button, Direction::Release).map_err(|e| e.to_string())
    }

    /// Drag the mouse from current position to the specified coordinates
    pub fn drag_to(&mut self, x: i32, y: i32, button_name: &str) -> Result<(), String> {
        // Press the mouse button
        self.press(button_name)?;

        // Small delay to ensure button press is registered
        sleep(Duration::from_millis(50));

        // Move to the target position
        self.move_to(x, y)?;

        // Small delay to ensure movement is registered
        sleep(Duration::from_millis(50));

        // Release the mouse button
        self.release(button_name)
    }
}
