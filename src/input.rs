use crossterm::event::{self, Event, KeyCode};
use std::time::Duration;

use crate::types::{AppEvent, EventSender};

/// Polls for keyboard input and terminal events
/// Sends quit event when 'q' is pressed and resize event when terminal is resized
pub fn keyboard_worker(tx: EventSender) {
    loop {
        // Poll every 200ms - humans won't notice the difference
        if event::poll(Duration::from_millis(200)).unwrap_or(false)
            && let Ok(event) = event::read() {
                match event {
                    Event::Key(key) => {
                        if key.code == KeyCode::Char('q') {
                            let _ = tx.blocking_send(AppEvent::Quit);
                            break;
                        }
                    }
                    Event::Resize(_, _) => {
                        let _ = tx.blocking_send(AppEvent::Resize);
                    }
                    _ => {}
                }
            }
    }
}
