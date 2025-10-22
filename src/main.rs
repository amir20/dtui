use bollard::container::{ListContainersOptions, StatsOptions};
use bollard::system::EventsOptions;
use bollard::Docker;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::stream::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Terminal,
};
use std::collections::HashMap;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
struct ContainerInfo {
    id: String,
    name: String,
    cpu: f64,
    status: String,
}

enum AppEvent {
    ContainerUpdate(String, ContainerInfo),
    ContainerRemoved(String),
    InitialContainerList(Vec<(String, ContainerInfo)>),
    Quit,
}

async fn stream_container_stats(
    docker: Docker,
    id: String,
    name: String,
    status: String,
    tx: mpsc::Sender<AppEvent>,
) {
    let stats_options = StatsOptions {
        stream: true,
        one_shot: false,
    };

    let mut stats_stream = docker.stats(&id, Some(stats_options));

    while let Some(result) = stats_stream.next().await {
        match result {
            Ok(stats) => {
                // Calculate CPU percentage
                let cpu_delta = stats.cpu_stats.cpu_usage.total_usage as f64
                    - stats.precpu_stats.cpu_usage.total_usage as f64;
                let system_delta = stats.cpu_stats.system_cpu_usage.unwrap_or(0) as f64
                    - stats.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;
                let number_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;

                let cpu_percent = if system_delta > 0.0 && cpu_delta > 0.0 {
                    (cpu_delta / system_delta) * number_cpus * 100.0
                } else {
                    0.0
                };

                let info = ContainerInfo {
                    id: id[..12.min(id.len())].to_string(),
                    name: name.clone(),
                    cpu: cpu_percent,
                    status: status.clone(),
                };

                if tx
                    .send(AppEvent::ContainerUpdate(id.clone(), info))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    // Notify that this container stream ended
    let _ = tx.send(AppEvent::ContainerRemoved(id)).await;
}

async fn container_manager(docker: Docker, tx: mpsc::Sender<AppEvent>) {
    let mut active_containers: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();

    // Fetch initial list of containers
    let list_options = Some(ListContainersOptions::<String> {
        all: false,
        ..Default::default()
    });

    if let Ok(container_list) = docker.list_containers(list_options).await {
        let mut initial_containers = Vec::new();

        for container in container_list {
            let id = container.id.clone().unwrap_or_default();
            let name = container
                .names
                .as_ref()
                .and_then(|n| n.first().map(|s| s.trim_start_matches('/').to_string()))
                .unwrap_or_default();
            let status = container.status.clone().unwrap_or_default();

            let initial_info = ContainerInfo {
                id: id[..12.min(id.len())].to_string(),
                name: name.clone(),
                cpu: 0.0,
                status: status.clone(),
            };

            initial_containers.push((id.clone(), initial_info));

            let tx_clone = tx.clone();
            let docker_clone = docker.clone();
            let id_clone = id.clone();

            let handle = tokio::spawn(async move {
                stream_container_stats(docker_clone, id_clone, name, status, tx_clone).await;
            });

            active_containers.insert(id, handle);
        }

        // Send all initial containers in one event
        if !initial_containers.is_empty() {
            let _ = tx
                .send(AppEvent::InitialContainerList(initial_containers))
                .await;
        }
    }

    // Subscribe to Docker events
    let mut filters = HashMap::new();
    filters.insert("type".to_string(), vec!["container".to_string()]);
    filters.insert(
        "event".to_string(),
        vec!["start".to_string(), "die".to_string(), "stop".to_string()],
    );

    let events_options = EventsOptions::<String> {
        filters,
        ..Default::default()
    };

    let mut events_stream = docker.events(Some(events_options));

    while let Some(event_result) = events_stream.next().await {
        match event_result {
            Ok(event) => {
                if let Some(actor) = event.actor {
                    let container_id = actor.id.unwrap_or_default();
                    let action = event.action.unwrap_or_default();

                    match action.as_str() {
                        "start" => {
                            // Get container details
                            if let Ok(inspect) = docker.inspect_container(&container_id, None).await
                            {
                                let name = inspect
                                    .name
                                    .as_ref()
                                    .map(|n| n.trim_start_matches('/').to_string())
                                    .unwrap_or_default();

                                let status = inspect
                                    .state
                                    .as_ref()
                                    .and_then(|s| s.status.as_ref())
                                    .map(|s| format!("{:?}", s))
                                    .unwrap_or_else(|| "running".to_string());

                                // Start monitoring the new container
                                if !active_containers.contains_key(&container_id) {
                                    let initial_info = ContainerInfo {
                                        id: container_id[..12.min(container_id.len())].to_string(),
                                        name: name.clone(),
                                        cpu: 0.0,
                                        status: status.clone(),
                                    };

                                    let _ = tx
                                        .send(AppEvent::InitialContainerList(vec![(
                                            container_id.clone(),
                                            initial_info,
                                        )]))
                                        .await;

                                    let tx_clone = tx.clone();
                                    let docker_clone = docker.clone();
                                    let id_clone = container_id.clone();

                                    let handle = tokio::spawn(async move {
                                        stream_container_stats(
                                            docker_clone,
                                            id_clone,
                                            name,
                                            status,
                                            tx_clone,
                                        )
                                        .await;
                                    });

                                    active_containers.insert(container_id, handle);
                                }
                            }
                        }
                        "die" | "stop" => {
                            // Stop monitoring and notify removal
                            if let Some(handle) = active_containers.remove(&container_id) {
                                handle.abort();
                                let _ = tx.send(AppEvent::ContainerRemoved(container_id)).await;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Err(_) => {
                // If event stream fails, wait and continue
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

fn keyboard_worker(tx: mpsc::Sender<AppEvent>) {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to Docker
    let docker = Docker::connect_with_local_defaults()?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create event channel
    let (tx, mut rx) = mpsc::channel::<AppEvent>(1000);

    // Spawn container manager
    let manager_tx = tx.clone();
    tokio::spawn(async move {
        container_manager(docker, manager_tx).await;
    });

    // Spawn keyboard worker in blocking thread
    let keyboard_tx = tx.clone();
    std::thread::spawn(move || {
        keyboard_worker(keyboard_tx);
    });

    let mut containers: HashMap<String, ContainerInfo> = HashMap::new();
    let mut should_quit = false;
    let mut last_draw = tokio::time::Instant::now();
    let draw_interval = Duration::from_millis(500); // Refresh UI every 500ms

    // Pre-allocate styles to avoid recreation every frame
    let style_high = Style::default().fg(Color::Red);
    let style_medium = Style::default().fg(Color::Yellow);
    let style_low = Style::default().fg(Color::Green);

    while !should_quit {
        // Process all pending events without blocking
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
                    should_quit = true;
                    events_processed = true;
                    break;
                }
                Err(_) => break, // No more events available
            }
        }

        // Only draw if enough time has passed or if we need to quit
        let now = tokio::time::Instant::now();
        if should_quit || now.duration_since(last_draw) >= draw_interval {
            terminal.draw(|f| {
                let size = f.area();

                // Collect references instead of cloning
                let mut container_refs: Vec<_> = containers.values().collect();
                container_refs.sort_by(|a, b| a.name.cmp(&b.name));

                let rows: Vec<Row> = container_refs
                    .iter()
                    .map(|c| {
                        let cpu_style = if c.cpu > 80.0 {
                            style_high
                        } else if c.cpu > 50.0 {
                            style_medium
                        } else {
                            style_low
                        };

                        Row::new(vec![
                            Cell::from(c.id.as_str()),
                            Cell::from(c.name.as_str()),
                            Cell::from(format!("{:.2}%", c.cpu)).style(cpu_style),
                            Cell::from(c.status.as_str()),
                        ])
                    })
                    .collect();

                let header = Row::new(vec!["Container ID", "Name", "CPU %", "Status"])
                    .style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                    .bottom_margin(1);

                let table = Table::new(
                    rows,
                    [
                        Constraint::Length(12),
                        Constraint::Min(20),
                        Constraint::Length(12),
                        Constraint::Length(15),
                    ],
                )
                .header(header)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!(
                            "Docker Container CPU Monitor - {} containers (Press 'q' to quit)",
                            containers.len()
                        ))
                        .style(Style::default().fg(Color::White)),
                );

                f.render_widget(table, size);
            })?;
            last_draw = now;
        }

        // If no events were processed, wait a bit to avoid busy-looping
        if !events_processed {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
