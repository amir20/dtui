mod docker;
mod input;
mod types;
mod ui;

use bollard::Docker;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::HashMap;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

use docker::container_manager;
use input::keyboard_worker;
use types::{AppEvent, ContainerInfo};
use ui::{render_ui, UiStyles};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Docker
    let docker = Docker::connect_with_local_defaults()?;

    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Create event channel
    let (tx, mut rx) = mpsc::channel::<AppEvent>(1000);

    // Spawn container manager
    spawn_container_manager(docker, tx.clone());

    // Spawn keyboard worker in blocking thread
    spawn_keyboard_worker(tx.clone());

    // Run main event loop
    run_event_loop(&mut terminal, &mut rx).await?;

    // Restore terminal
    cleanup_terminal(&mut terminal)?;

    Ok(())
}

/// Sets up the terminal for TUI rendering
fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

/// Restores the terminal to its original state
fn cleanup_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Spawns the container manager task
fn spawn_container_manager(docker: Docker, tx: mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        container_manager(docker, tx).await;
    });
}

/// Spawns the keyboard input worker thread
fn spawn_keyboard_worker(tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        keyboard_worker(tx);
    });
}

/// Main event loop that processes events and renders the UI
async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    rx: &mut mpsc::Receiver<AppEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut containers: HashMap<String, ContainerInfo> = HashMap::new();
    let mut should_quit = false;
    let mut last_draw = tokio::time::Instant::now();
    let draw_interval = Duration::from_millis(500); // Refresh UI every 500ms

    // Pre-allocate styles to avoid recreation every frame
    let styles = UiStyles::default();

    while !should_quit {
        // Process all pending events without blocking
        let events_processed = process_events(rx, &mut containers, &mut should_quit);

        // Only draw if enough time has passed or if we need to quit
        let now = tokio::time::Instant::now();
        if should_quit || now.duration_since(last_draw) >= draw_interval {
            terminal.draw(|f| {
                render_ui(f, &containers, &styles);
            })?;
            last_draw = now;
        }

        // If no events were processed, wait a bit to avoid busy-looping
        if !events_processed {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    Ok(())
}

/// Processes all pending events from the event channel
fn process_events(
    rx: &mut mpsc::Receiver<AppEvent>,
    containers: &mut HashMap<String, ContainerInfo>,
    should_quit: &mut bool,
) -> bool {
    let mut events_processed = false;

    loop {
        match rx.try_recv() {
            Ok(AppEvent::ContainerUpdate(id, info)) => {
                containers.insert(id, info);
                events_processed = true;
            }
            Ok(AppEvent::ContainerRemoved(id)) => {
                containers.remove(&id);
                events_processed = true;
            }
            Ok(AppEvent::InitialContainerList(container_list)) => {
                for (id, info) in container_list {
                    containers.insert(id, info);
                }
                events_processed = true;
            }
            Ok(AppEvent::Quit) => {
                *should_quit = true;
                events_processed = true;
                break;
            }
            Err(_) => break, // No more events available
        }
    }

    events_processed
}
