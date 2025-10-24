use bollard::models::ContainerStatsResponse;
use bollard::query_parameters::{
    EventsOptions, InspectContainerOptions, ListContainersOptions, StatsOptions,
};
use bollard::Docker;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::time::Duration;

use crate::types::{AppEvent, Container, ContainerStats, EventSender};

/// Streams stats for a single container and sends updates via the event channel
pub async fn stream_container_stats(docker: Docker, id: String, tx: EventSender) {
    let stats_options = StatsOptions {
        stream: true,
        one_shot: false,
    };

    let mut stats_stream = docker.stats(&id, Some(stats_options));

    while let Some(result) = stats_stream.next().await {
        match result {
            Ok(stats) => {
                let cpu_percent = calculate_cpu_percentage(&stats);
                let memory_percent = calculate_memory_percentage(&stats);

                let stats = ContainerStats {
                    cpu: cpu_percent,
                    memory: memory_percent,
                };

                if tx
                    .send(AppEvent::ContainerStat(id.clone(), stats))
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
    let _ = tx.send(AppEvent::ContainerDestroyed(id)).await;
}

/// Calculates CPU usage percentage from container stats
fn calculate_cpu_percentage(stats: &ContainerStatsResponse) -> f64 {
    let cpu_stats = match &stats.cpu_stats {
        Some(cs) => cs,
        None => return 0.0,
    };
    let precpu_stats = match &stats.precpu_stats {
        Some(pcs) => pcs,
        None => return 0.0,
    };

    let cpu_usage = cpu_stats
        .cpu_usage
        .as_ref()
        .and_then(|u| u.total_usage)
        .unwrap_or(0);
    let precpu_usage = precpu_stats
        .cpu_usage
        .as_ref()
        .and_then(|u| u.total_usage)
        .unwrap_or(0);
    let cpu_delta = cpu_usage as f64 - precpu_usage as f64;

    let system_delta = cpu_stats.system_cpu_usage.unwrap_or(0) as f64
        - precpu_stats.system_cpu_usage.unwrap_or(0) as f64;
    let number_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;

    if system_delta > 0.0 && cpu_delta > 0.0 {
        (cpu_delta / system_delta) * number_cpus * 100.0
    } else {
        0.0
    }
}

/// Calculates memory usage percentage from container stats
fn calculate_memory_percentage(stats: &ContainerStatsResponse) -> f64 {
    let memory_stats = match &stats.memory_stats {
        Some(ms) => ms,
        None => return 0.0,
    };

    let memory_usage = memory_stats.usage.unwrap_or(0) as f64;
    let memory_limit = memory_stats.limit.unwrap_or(1) as f64;

    if memory_limit > 0.0 {
        (memory_usage / memory_limit) * 100.0
    } else {
        0.0
    }
}

/// Manages container monitoring: fetches initial containers and listens for Docker events
pub async fn container_manager(docker: Docker, tx: EventSender) {
    let mut active_containers: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();

    // Fetch and start monitoring initial containers
    fetch_initial_containers(&docker, &tx, &mut active_containers).await;

    // Subscribe to Docker events and handle container lifecycle
    monitor_docker_events(&docker, &tx, &mut active_containers).await;
}

/// Fetches the initial list of running containers and starts monitoring them
async fn fetch_initial_containers(
    docker: &Docker,
    tx: &EventSender,
    active_containers: &mut HashMap<String, tokio::task::JoinHandle<()>>,
) {
    let list_options = Some(ListContainersOptions {
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

            let container_info = Container {
                id: id[..12.min(id.len())].to_string(),
                name: name.clone(),
                status: status.clone(),
                stats: ContainerStats::default(),
            };

            initial_containers.push(container_info);

            start_container_monitoring(docker, &id, tx, active_containers);
        }

        // Send all initial containers in one event
        if !initial_containers.is_empty() {
            let _ = tx
                .send(AppEvent::InitialContainerList(initial_containers))
                .await;
        }
    }
}

/// Monitors Docker events for container start/stop/die events
async fn monitor_docker_events(
    docker: &Docker,
    tx: &EventSender,
    active_containers: &mut HashMap<String, tokio::task::JoinHandle<()>>,
) {
    let mut filters = HashMap::new();
    filters.insert("type".to_string(), vec!["container".to_string()]);
    filters.insert(
        "event".to_string(),
        vec!["start".to_string(), "die".to_string(), "stop".to_string()],
    );

    let events_options = EventsOptions {
        filters: Some(filters),
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
                            handle_container_start(docker, &container_id, tx, active_containers)
                                .await;
                        }
                        "die" | "stop" => {
                            handle_container_stop(&container_id, tx, active_containers).await;
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

/// Starts monitoring a container by spawning a stats stream task
fn start_container_monitoring(
    docker: &Docker,
    container_id: &str,
    tx: &EventSender,
    active_containers: &mut HashMap<String, tokio::task::JoinHandle<()>>,
) {
    let tx_clone = tx.clone();
    let docker_clone = docker.clone();
    let id_clone = container_id.to_string();

    let handle = tokio::spawn(async move {
        stream_container_stats(docker_clone, id_clone, tx_clone).await;
    });

    active_containers.insert(container_id.to_string(), handle);
}

/// Handles a container start event
async fn handle_container_start(
    docker: &Docker,
    container_id: &str,
    tx: &EventSender,
    active_containers: &mut HashMap<String, tokio::task::JoinHandle<()>>,
) {
    // Get container details
    if let Ok(inspect) = docker
        .inspect_container(container_id, None::<InspectContainerOptions>)
        .await
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
        if !active_containers.contains_key(container_id) {
            let container = Container {
                id: container_id[..12.min(container_id.len())].to_string(),
                name: name.clone(),
                status: status.clone(),
                stats: ContainerStats::default(),
            };

            let _ = tx.send(AppEvent::ContainerCreated(container)).await;

            start_container_monitoring(docker, container_id, tx, active_containers);
        }
    }
}

/// Handles a container stop/die event
async fn handle_container_stop(
    container_id: &str,
    tx: &EventSender,
    active_containers: &mut HashMap<String, tokio::task::JoinHandle<()>>,
) {
    // Stop monitoring and notify removal
    if let Some(handle) = active_containers.remove(container_id) {
        handle.abort();
        let _ = tx
            .send(AppEvent::ContainerDestroyed(container_id.to_string()))
            .await;
    }
}
