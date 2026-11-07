//! Window placement. No custom frame on macOS: the Slint window keeps its own
//! decorations and move area, and mouse back/forward buttons are not wired.

use super::WindowPlacement;

pub fn allow_dark_menus() {}

pub fn window_placement(window: &slint::Window) -> Option<WindowPlacement> {
    let size = window.size();
    if size.width == 0 || size.height == 0 {
        return None;
    }
    let position = window.position();
    Some(WindowPlacement {
        rect: [position.x, position.y, position.x + size.width as i32, position.y + size.height as i32],
        maximized: false,
    })
}

pub fn apply_window_placement(window: &slint::Window, saved: WindowPlacement) {
    let (width, height) = saved.size();
    window.set_size(slint::PhysicalSize::new(width, height));
    window.set_position(slint::PhysicalPosition::new(saved.rect[0], saved.rect[1]));
}

pub fn restore_frame_appearance(_window: &slint::Window, _on_navigate: impl Fn(bool) + 'static) {}
