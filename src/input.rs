use crossterm::event::{self, Event, KeyCode};
use std::time::Duration;

use crate::types::{AppEvent, EventSender};

/// Polls for keyboard input and sends quit event when 'q' is pressed
pub fn keyboard_worker(tx: EventSender) {
    loop {
        // Poll every 200ms - humans won't notice the difference
        if event::poll(Duration::from_millis(200)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.code == KeyCode::Char('q') {
                    let _ = tx.blocking_send(AppEvent::Quit);
                    break;
                }
            }
        }
    }
}
