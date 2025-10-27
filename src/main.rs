mod config;
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

use config::Config;
use docker::{DockerHost, container_manager};
use input::keyboard_worker;
use types::{AppEvent, Container, ContainerKey};
use ui::{UiStyles, render_ui};

/// Docker container monitoring TUI
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Docker host(s) to connect to. Can be specified multiple times.
    ///
    /// Examples:
    ///   --host local                    (Connect to local Docker daemon)
    ///   --host ssh://user@host          (Connect via SSH)
    ///   --host ssh://user@host:2222     (Connect via SSH with custom port)
    ///   --host tcp://host:2375          (Connect via TCP to remote Docker daemon)
    ///   --host local --host ssh://user@server1 --host tcp://server2:2375  (Multiple hosts)
    ///
    /// If not specified, will use config file or default to "local"
    #[arg(short = 'H', long)]
    host: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();

    // Load config file if it exists
    let config = Config::load()?.unwrap_or_default();

    // Determine if CLI hosts were explicitly provided
    let cli_provided = !args.host.is_empty();

    // Merge config with CLI args (CLI takes precedence)
    let merged_config = if cli_provided {
        // User explicitly provided --host, use CLI args
        config.merge_with_cli_hosts(args.host.clone(), false)
    } else if !config.hosts.is_empty() {
        // No CLI args but config has hosts, use config
        config
    } else {
        // Neither CLI nor config provided hosts, use default "local"
        config.merge_with_cli_hosts(vec!["local".to_string()], true)
    };

    // Get final list of hosts
    let hosts: Vec<String> = if merged_config.hosts.is_empty() {
        vec!["local".to_string()]
    } else {
        merged_config
            .host_strings()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    };

    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Create event channel
    let (tx, mut rx) = mpsc::channel::<AppEvent>(1000);

    // Connect to all hosts and spawn container managers
    for host_spec in hosts {
        match connect_docker(&host_spec) {
            Ok(docker) => {
                // Create a unique host ID from the host spec
                let host_id = create_host_id(&host_spec);
                let docker_host = DockerHost::new(host_id.clone(), docker);

                // Spawn container manager for this host
                spawn_container_manager(docker_host, tx.clone());
            }
            Err(e) => {
                // Log error but continue with other hosts
                eprintln!("Failed to connect to host '{}': {}", host_spec, e);
            }
        }
    }

    // Spawn keyboard worker in blocking thread
    spawn_keyboard_worker(tx.clone());

    // Run main event loop
    run_event_loop(&mut terminal, &mut rx).await?;

    // Restore terminal
    cleanup_terminal(&mut terminal)?;

    Ok(())
}

/// Creates a unique host identifier from the host specification
fn create_host_id(host_spec: &str) -> String {
    if host_spec == "local" {
        "local".to_string()
    } else if host_spec.starts_with("ssh://") {
        // Extract user@host from ssh://user@host[:port]
        host_spec
            .strip_prefix("ssh://")
            .unwrap_or(host_spec)
            .split(':')
            .next()
            .unwrap_or(host_spec)
            .to_string()
    } else if host_spec.starts_with("tcp://") {
        // Extract host:port from tcp://host:port
        host_spec
            .strip_prefix("tcp://")
            .unwrap_or(host_spec)
            .to_string()
    } else {
        host_spec.to_string()
    }
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
    } else if host.starts_with("tcp://") {
        // Connect via TCP (remote Docker daemon)
        Ok(Docker::connect_with_http(
            host,
            120, // timeout in seconds
            API_DEFAULT_VERSION,
        )?)
    } else {
        Err(format!(
            "Invalid host format: '{}'. Use 'local', 'ssh://user@host[:port]', or 'tcp://host:port'",
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

/// Spawns the container manager task for a specific host
fn spawn_container_manager(docker_host: DockerHost, tx: mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        container_manager(docker_host, tx).await;
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
    // Use ContainerKey (host_id, container_id) as the key
    let mut containers: HashMap<ContainerKey, Container> = HashMap::new();
    let mut sorted_container_keys: Vec<ContainerKey> = Vec::new(); // Pre-sorted list of container keys
    let mut should_quit = false;
    let mut table_state = TableState::default();
    let draw_interval = Duration::from_millis(500); // Refresh UI every 500ms
    let mut last_draw = std::time::Instant::now();

    // Pre-allocate styles to avoid recreation every frame
    let styles = UiStyles::default();

    while !should_quit {
        // Wait for events with timeout - handles both throttling and waiting
        let force_draw = process_events(
            rx,
            &mut containers,
            &mut sorted_container_keys,
            &mut should_quit,
            &mut table_state,
            draw_interval,
        )
        .await;

        // Draw UI if forced (table structure changed) or if draw_interval has elapsed
        let should_draw = force_draw || last_draw.elapsed() >= draw_interval;

        if should_draw {
            terminal.draw(|f| {
                render_ui(
                    f,
                    &containers,
                    &sorted_container_keys,
                    &styles,
                    &mut table_state,
                );
            })?;
            last_draw = std::time::Instant::now();
        }
    }

    Ok(())
}

/// Processes all pending events from the event channel
/// Waits with timeout for at least one event, then drains all pending events
/// Returns true if a force draw is needed (table structure changed)
async fn process_events(
    rx: &mut mpsc::Receiver<AppEvent>,
    containers: &mut HashMap<ContainerKey, Container>,
    sorted_container_keys: &mut Vec<ContainerKey>,
    should_quit: &mut bool,
    table_state: &mut TableState,
    timeout: Duration,
) -> bool {
    let mut force_draw = false;

    // Wait for first event with timeout
    match tokio::time::timeout(timeout, rx.recv()).await {
        Ok(Some(event)) => {
            force_draw |= process_single_event(
                event,
                containers,
                sorted_container_keys,
                should_quit,
                table_state,
            );
        }
        Ok(None) => {
            // Channel closed
            *should_quit = true;
            return false;
        }
        Err(_) => {
            // Timeout - no events, just return without forcing draw
            return false;
        }
    }

    // Drain any additional pending events without blocking
    while let Ok(event) = rx.try_recv() {
        force_draw |= process_single_event(
            event,
            containers,
            sorted_container_keys,
            should_quit,
            table_state,
        );
    }

    force_draw
}

/// Processes a single event
/// Returns true if this event requires an immediate draw (table structure changed)
fn process_single_event(
    event: AppEvent,
    containers: &mut HashMap<ContainerKey, Container>,
    sorted_container_keys: &mut Vec<ContainerKey>,
    should_quit: &mut bool,
    table_state: &mut TableState,
) -> bool {
    match event {
        AppEvent::InitialContainerList(host_id, container_list) => {
            for container in container_list {
                let key = ContainerKey::new(host_id.clone(), container.id.clone());
                containers.insert(key.clone(), container);
                sorted_container_keys.push(key);
            }
            // Sort once after adding all initial containers
            // Sort by host_id first, then by container name
            sorted_container_keys.sort_by(|a, b| {
                let container_a = containers.get(a).unwrap();
                let container_b = containers.get(b).unwrap();

                // First compare by host_id
                match container_a.host_id.cmp(&container_b.host_id) {
                    std::cmp::Ordering::Equal => {
                        // If same host, compare by name
                        container_a.name.cmp(&container_b.name)
                    }
                    other => other,
                }
            });
            // Select first row if we have containers
            if !containers.is_empty() {
                table_state.select(Some(0));
            }
            true // Force draw - table structure changed
        }
        AppEvent::ContainerCreated(container) => {
            let key = ContainerKey::new(container.host_id.clone(), container.id.clone());
            let sort_key = (container.host_id.clone(), container.name.clone());
            containers.insert(key.clone(), container);

            // Insert into sorted position (by host_id, then name)
            let insert_pos = sorted_container_keys
                .binary_search_by(|probe_key| {
                    let probe_container = containers.get(probe_key).unwrap();
                    let probe_sort_key = (&probe_container.host_id, &probe_container.name);
                    probe_sort_key.cmp(&(&sort_key.0, &sort_key.1))
                })
                .unwrap_or_else(|pos| pos);
            sorted_container_keys.insert(insert_pos, key);

            // Select first row if this is the first container
            if containers.len() == 1 {
                table_state.select(Some(0));
            }
            true // Force draw - table structure changed
        }
        AppEvent::ContainerDestroyed(key) => {
            containers.remove(&key);
            sorted_container_keys.retain(|k| k != &key);

            // Adjust selection if needed
            let container_count = containers.len();
            if container_count == 0 {
                table_state.select(None);
            } else if let Some(selected) = table_state.selected()
                && selected >= container_count
            {
                table_state.select(Some(container_count - 1));
            }
            true // Force draw - table structure changed
        }
        AppEvent::ContainerStat(key, stats) => {
            // Update stats on existing container
            if let Some(container) = containers.get_mut(&key) {
                container.stats = stats;
            }
            false // No force draw - just stats update
        }
        AppEvent::Resize => {
            // Just process the event - UI will redraw immediately after
            true // Force draw - UI needs to adjust to new size
        }
        AppEvent::Quit => {
            *should_quit = true;
            false // No need to draw when quitting
        }
        AppEvent::SelectPrevious => {
            let container_count = containers.len();
            if container_count > 0 {
                let selected = table_state.selected().unwrap_or(0);
                if selected > 0 {
                    table_state.select(Some(selected - 1));
                }
            }
            true // Force draw - selection changed
        }
        AppEvent::SelectNext => {
            let container_count = containers.len();
            if container_count > 0 {
                let selected = table_state.selected().unwrap_or(0);
                if selected < container_count - 1 {
                    table_state.select(Some(selected + 1));
                }
            }
            true // Force draw - selection changed
        }
    }
}
