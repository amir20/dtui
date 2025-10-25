mod docker;
mod input;
mod types;
mod ui;

use bollard::{API_DEFAULT_VERSION, Docker};
use clap::Parser;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::TableState};
use std::collections::HashMap;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

use docker::container_manager;
use input::keyboard_worker;
use types::{AppEvent, Container};
use ui::{UiStyles, render_ui};

/// Docker container monitoring TUI
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Docker host to connect to
    ///
    /// Examples:
    ///   --host local                    (Connect to local Docker daemon)
    ///   --host ssh://user@host          (Connect via SSH)
    ///   --host ssh://user@host:2222     (Connect via SSH with custom port)
    #[arg(short = 'H', long, default_value = "local")]
    host: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();

    // Connect to Docker based on the host argument
    let docker = connect_docker(&args.host)?;

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

/// Connects to Docker based on the host string
fn connect_docker(host: &str) -> Result<Docker, Box<dyn std::error::Error>> {
    if host == "local" {
        // Connect to local Docker daemon using default settings
        Ok(Docker::connect_with_local_defaults()?)
    } else if host.starts_with("ssh://") {
        // Connect via SSH with 120 second timeout
        Ok(Docker::connect_with_ssh(
            host,
            120, // timeout in seconds
            API_DEFAULT_VERSION,
        )?)
    } else {
        Err(format!(
            "Invalid host format: '{}'. Use 'local' or 'ssh://user@host[:port]'",
            host
        )
        .into())
    }
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
    let mut containers: HashMap<String, Container> = HashMap::new();
    let mut should_quit = false;
    let mut table_state = TableState::default();
    let draw_interval = Duration::from_millis(500); // Refresh UI every 500ms

    // Pre-allocate styles to avoid recreation every frame
    let styles = UiStyles::default();

    while !should_quit {
        // Wait for events with timeout - handles both throttling and waiting
        process_events(
            rx,
            &mut containers,
            &mut should_quit,
            &mut table_state,
            draw_interval,
        )
        .await;

        // Draw UI after processing events
        terminal.draw(|f| {
            render_ui(f, &containers, &styles, &mut table_state);
        })?;
    }

    Ok(())
}

/// Processes all pending events from the event channel
/// Waits with timeout for at least one event, then drains all pending events
async fn process_events(
    rx: &mut mpsc::Receiver<AppEvent>,
    containers: &mut HashMap<String, Container>,
    should_quit: &mut bool,
    table_state: &mut TableState,
    timeout: Duration,
) {
    // Wait for first event with timeout
    match tokio::time::timeout(timeout, rx.recv()).await {
        Ok(Some(event)) => {
            process_single_event(event, containers, should_quit, table_state);
        }
        Ok(None) => {
            // Channel closed
            *should_quit = true;
            return;
        }
        Err(_) => {
            // Timeout - no events, just return to redraw
            return;
        }
    }

    // Drain any additional pending events without blocking
    while let Ok(event) = rx.try_recv() {
        process_single_event(event, containers, should_quit, table_state);
    }
}

/// Processes a single event
fn process_single_event(
    event: AppEvent,
    containers: &mut HashMap<String, Container>,
    should_quit: &mut bool,
    table_state: &mut TableState,
) {
    match event {
        AppEvent::InitialContainerList(container_list) => {
            for container in container_list {
                containers.insert(container.id.clone(), container);
            }
            // Select first row if we have containers
            if !containers.is_empty() {
                table_state.select(Some(0));
            }
        }
        AppEvent::ContainerCreated(container) => {
            containers.insert(container.id.clone(), container);
            // Select first row if this is the first container
            if containers.len() == 1 {
                table_state.select(Some(0));
            }
        }
        AppEvent::ContainerDestroyed(id) => {
            containers.remove(&id);
            // Adjust selection if needed
            let container_count = containers.len();
            if container_count == 0 {
                table_state.select(None);
            } else if let Some(selected) = table_state.selected()
                && selected >= container_count {
                    table_state.select(Some(container_count - 1));
                }
        }
        AppEvent::ContainerStat(id, stats) => {
            // Update stats on existing container
            if let Some(container) = containers.get_mut(&id) {
                container.stats = stats;
            }
        }
        AppEvent::Resize => {
            // Just process the event - UI will redraw immediately after
        }
        AppEvent::Quit => {
            *should_quit = true;
        }
        AppEvent::SelectPrevious => {
            let container_count = containers.len();
            if container_count > 0 {
                let selected = table_state.selected().unwrap_or(0);
                if selected > 0 {
                    table_state.select(Some(selected - 1));
                }
            }
        }
        AppEvent::SelectNext => {
            let container_count = containers.len();
            if container_count > 0 {
                let selected = table_state.selected().unwrap_or(0);
                if selected < container_count - 1 {
                    table_state.select(Some(selected + 1));
                }
            }
        }
    }
}
