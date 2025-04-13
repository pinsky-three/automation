use enigo::{Direction, Enigo, Key, Keyboard as EnigoKeyboard};
use std::thread::sleep;
use std::time::Duration;

/// Keyboard provides methods for keyboard control
pub struct Keyboard<'a> {
    enigo: &'a mut Enigo,
}

impl<'a> Keyboard<'a> {
    /// Create a new Keyboard with a reference to an Enigo instance
    pub fn new(enigo: &'a mut Enigo) -> Self {
        Keyboard { enigo }
    }

    /// Press a single key
    pub fn press_key(&mut self, key_name: &str) -> Result<(), String> {
        match key_name.to_lowercase().as_str() {
            "return" | "enter" => EnigoKeyboard::key(self.enigo, Key::Return, Direction::Click)
                .map_err(|e| e.to_string()),
            "tab" => EnigoKeyboard::key(self.enigo, Key::Tab, Direction::Click)
                .map_err(|e| e.to_string()),
            "escape" => EnigoKeyboard::key(self.enigo, Key::Escape, Direction::Click)
                .map_err(|e| e.to_string()),
            _ => Err(format!("Unknown key: {}", key_name)),
        }
    }

    /// Type text
    pub fn type_text(&mut self, text: &str) -> Result<(), String> {
        EnigoKeyboard::text(self.enigo, text).map_err(|e| e.to_string())
    }

    /// Press a key combination
    pub fn press_key_combination(&mut self, keys: &[String]) -> Result<(), String> {
        if keys.is_empty() {
            return Err("No keys provided for combination".to_string());
        }

        // Press all modifier keys first
        for key in &keys[0..keys.len() - 1] {
            match key.to_lowercase().as_str() {
                "control" | "ctrl" => {
                    EnigoKeyboard::key(self.enigo, Key::Control, Direction::Press)
                        .map_err(|e| e.to_string())?;
                }
                "cmd" => {
                    EnigoKeyboard::key(self.enigo, Key::Meta, Direction::Press)
                        .map_err(|e| e.to_string())?;
                }
                "alt" => {
                    EnigoKeyboard::key(self.enigo, Key::Alt, Direction::Press)
                        .map_err(|e| e.to_string())?;
                }
                "shift" => {
                    EnigoKeyboard::key(self.enigo, Key::Shift, Direction::Press)
                        .map_err(|e| e.to_string())?;
                }
                "meta" | "super" | "windows" => {
                    EnigoKeyboard::key(self.enigo, Key::Meta, Direction::Press)
                        .map_err(|e| e.to_string())?;
                }
                _ => return Err(format!("Unknown modifier key: {}", key)),
            }
        }

        // Small delay to ensure modifier keys are registered
        sleep(Duration::from_millis(50));

        // Press the last key (non-modifier)
        if let Some(last_key) = keys.last() {
            match last_key.to_lowercase().as_str() {
                "t" => EnigoKeyboard::text(self.enigo, "t").map_err(|e| e.to_string())?,
                "w" => EnigoKeyboard::text(self.enigo, "w").map_err(|e| e.to_string())?,
                "r" => EnigoKeyboard::text(self.enigo, "r").map_err(|e| e.to_string())?,
                "l" => EnigoKeyboard::text(self.enigo, "l").map_err(|e| e.to_string())?,
                "a" => EnigoKeyboard::text(self.enigo, "a").map_err(|e| e.to_string())?,
                "c" => EnigoKeyboard::text(self.enigo, "c").map_err(|e| e.to_string())?,
                "v" => EnigoKeyboard::text(self.enigo, "v").map_err(|e| e.to_string())?,
                "x" => EnigoKeyboard::text(self.enigo, "x").map_err(|e| e.to_string())?,
                "z" => EnigoKeyboard::text(self.enigo, "z").map_err(|e| e.to_string())?,
                _ => return Err(format!("Unknown key in combination: {}", last_key)),
            }
        }

        // Small delay to ensure the key combination is registered
        sleep(Duration::from_millis(50));

        // Release all modifier keys in reverse order
        for key in keys[0..keys.len() - 1].iter().rev() {
            match key.to_lowercase().as_str() {
                "control" | "ctrl" => {
                    EnigoKeyboard::key(self.enigo, Key::Control, Direction::Release)
                        .map_err(|e| e.to_string())?;
                }
                "cmd" => {
                    EnigoKeyboard::key(self.enigo, Key::Meta, Direction::Release)
                        .map_err(|e| e.to_string())?;
                }
                "alt" => {
                    EnigoKeyboard::key(self.enigo, Key::Alt, Direction::Release)
                        .map_err(|e| e.to_string())?;
                }
                "shift" => {
                    EnigoKeyboard::key(self.enigo, Key::Shift, Direction::Release)
                        .map_err(|e| e.to_string())?;
                }
                "meta" | "super" | "windows" => {
                    EnigoKeyboard::key(self.enigo, Key::Meta, Direction::Release)
                        .map_err(|e| e.to_string())?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Alt+Tab window switching
    pub fn alt_tab(&mut self) -> Result<(), String> {
        EnigoKeyboard::key(self.enigo, Key::Alt, Direction::Press).map_err(|e| e.to_string())?;
        sleep(Duration::from_millis(100));
        EnigoKeyboard::key(self.enigo, Key::Tab, Direction::Click).map_err(|e| e.to_string())?;
        sleep(Duration::from_millis(100));
        EnigoKeyboard::key(self.enigo, Key::Alt, Direction::Release).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Command+Tab window switching (for macOS)
    pub fn cmd_tab(&mut self) -> Result<(), String> {
        EnigoKeyboard::key(self.enigo, Key::Meta, Direction::Press).map_err(|e| e.to_string())?;
        sleep(Duration::from_millis(100));
        EnigoKeyboard::key(self.enigo, Key::Tab, Direction::Click).map_err(|e| e.to_string())?;
        sleep(Duration::from_millis(100));
        EnigoKeyboard::key(self.enigo, Key::Meta, Direction::Release).map_err(|e| e.to_string())?;
        Ok(())
    }
}
