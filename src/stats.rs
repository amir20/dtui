use bollard::models::ContainerStatsResponse;
use bollard::query_parameters::StatsOptions;
use futures_util::stream::StreamExt;

use crate::docker::DockerHost;
use crate::types::{AppEvent, ContainerKey, ContainerStats, EventSender};

/// Streams stats for a single container and sends updates via the event channel
///
/// Uses exponential decay smoothing to reduce noise in stats:
/// smoothed = alpha * new_value + (1 - alpha) * previous_smoothed
///
/// # Arguments
/// * `host` - Docker host instance with identifier
/// * `truncated_id` - Truncated container ID (12 chars) - Docker API accepts partial IDs
/// * `tx` - Event sender channel
pub async fn stream_container_stats(host: DockerHost, truncated_id: String, tx: EventSender) {
    let stats_options = StatsOptions {
        stream: true,
        one_shot: false,
    };

    let mut stats_stream = host.docker.stats(&truncated_id, Some(stats_options));

    // Smoothing factor: higher alpha = more responsive, lower alpha = smoother
    // 0.3 provides good balance between responsiveness and smoothness
    const ALPHA: f64 = 0.3;

    let mut smoothed_cpu: Option<f64> = None;
    let mut smoothed_memory: Option<f64> = None;

    while let Some(result) = stats_stream.next().await {
        match result {
            Ok(stats) => {
                let cpu_percent = calculate_cpu_percentage(&stats);
                let memory_percent = calculate_memory_percentage(&stats);

                // Apply exponential moving average
                let cpu = match smoothed_cpu {
                    Some(prev) => ALPHA * cpu_percent + (1.0 - ALPHA) * prev,
                    None => cpu_percent, // First value, no smoothing
                };

                let memory = match smoothed_memory {
                    Some(prev) => ALPHA * memory_percent + (1.0 - ALPHA) * prev,
                    None => memory_percent, // First value, no smoothing
                };

                // Update smoothed values for next iteration
                smoothed_cpu = Some(cpu);
                smoothed_memory = Some(memory);

                let stats = ContainerStats { cpu, memory };

                let key = ContainerKey::new(host.host_id.clone(), truncated_id.clone());
                if tx.send(AppEvent::ContainerStat(key, stats)).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    // Notify that this container stream ended
    let key = ContainerKey::new(host.host_id, truncated_id);
    let _ = tx.send(AppEvent::ContainerDestroyed(key)).await;
}

/// Calculates CPU usage percentage from container stats
pub fn calculate_cpu_percentage(stats: &ContainerStatsResponse) -> f64 {
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
pub fn calculate_memory_percentage(stats: &ContainerStatsResponse) -> f64 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bollard::models::{ContainerCpuStats, ContainerCpuUsage, ContainerMemoryStats};

    fn create_cpu_stats(
        total_usage: u64,
        system_cpu_usage: u64,
        online_cpus: u32,
    ) -> ContainerCpuStats {
        ContainerCpuStats {
            cpu_usage: Some(ContainerCpuUsage {
                total_usage: Some(total_usage),
                percpu_usage: None,
                usage_in_kernelmode: None,
                usage_in_usermode: None,
            }),
            system_cpu_usage: Some(system_cpu_usage),
            online_cpus: Some(online_cpus),
            throttling_data: None,
        }
    }

    #[test]
    fn test_calculate_cpu_percentage_normal_usage() {
        let stats = ContainerStatsResponse {
            cpu_stats: Some(create_cpu_stats(1_000_000_000, 2_000_000_000, 4)),
            precpu_stats: Some(create_cpu_stats(500_000_000, 1_000_000_000, 4)),
            ..Default::default()
        };

        let cpu = calculate_cpu_percentage(&stats);

        // CPU delta: 1B - 500M = 500M
        // System delta: 2B - 1B = 1B
        // (500M / 1B) * 4 CPUs * 100 = 200%
        assert_eq!(cpu, 200.0);
    }

    #[test]
    fn test_calculate_cpu_percentage_single_core() {
        let stats = ContainerStatsResponse {
            cpu_stats: Some(create_cpu_stats(800_000_000, 1_000_000_000, 1)),
            precpu_stats: Some(create_cpu_stats(200_000_000, 500_000_000, 1)),
            ..Default::default()
        };

        let cpu = calculate_cpu_percentage(&stats);

        // CPU delta: 800M - 200M = 600M
        // System delta: 1B - 500M = 500M
        // (600M / 500M) * 1 CPU * 100 = 120%
        assert_eq!(cpu, 120.0);
    }

    #[test]
    fn test_calculate_cpu_percentage_missing_cpu_stats() {
        let stats = ContainerStatsResponse {
            cpu_stats: None,
            precpu_stats: None,
            ..Default::default()
        };

        assert_eq!(calculate_cpu_percentage(&stats), 0.0);
    }

    #[test]
    fn test_calculate_cpu_percentage_missing_precpu_stats() {
        let stats = ContainerStatsResponse {
            cpu_stats: Some(create_cpu_stats(1_000_000_000, 2_000_000_000, 4)),
            precpu_stats: None,
            ..Default::default()
        };

        assert_eq!(calculate_cpu_percentage(&stats), 0.0);
    }

    #[test]
    fn test_calculate_cpu_percentage_zero_system_delta() {
        let stats = ContainerStatsResponse {
            cpu_stats: Some(create_cpu_stats(1_000_000_000, 2_000_000_000, 4)),
            precpu_stats: Some(create_cpu_stats(500_000_000, 2_000_000_000, 4)), // Same system CPU
            ..Default::default()
        };

        // Should return 0.0 when system delta is 0
        assert_eq!(calculate_cpu_percentage(&stats), 0.0);
    }

    #[test]
    fn test_calculate_cpu_percentage_zero_cpu_delta() {
        let stats = ContainerStatsResponse {
            cpu_stats: Some(create_cpu_stats(1_000_000_000, 2_000_000_000, 4)),
            precpu_stats: Some(create_cpu_stats(1_000_000_000, 1_000_000_000, 4)), // Same CPU usage
            ..Default::default()
        };

        // Should return 0.0 when CPU delta is 0
        assert_eq!(calculate_cpu_percentage(&stats), 0.0);
    }

    #[test]
    fn test_calculate_memory_percentage_normal_usage() {
        let stats = ContainerStatsResponse {
            memory_stats: Some(ContainerMemoryStats {
                usage: Some(500_000_000),   // 500 MB
                limit: Some(1_000_000_000), // 1 GB
                max_usage: None,
                stats: None,
                failcnt: None,
                commitbytes: None,
                commitpeakbytes: None,
                privateworkingset: None,
            }),
            ..Default::default()
        };

        assert_eq!(calculate_memory_percentage(&stats), 50.0);
    }

    #[test]
    fn test_calculate_memory_percentage_full_usage() {
        let stats = ContainerStatsResponse {
            memory_stats: Some(ContainerMemoryStats {
                usage: Some(1_000_000_000),
                limit: Some(1_000_000_000),
                max_usage: None,
                stats: None,
                failcnt: None,
                commitbytes: None,
                commitpeakbytes: None,
                privateworkingset: None,
            }),
            ..Default::default()
        };

        assert_eq!(calculate_memory_percentage(&stats), 100.0);
    }

    #[test]
    fn test_calculate_memory_percentage_low_usage() {
        let stats = ContainerStatsResponse {
            memory_stats: Some(ContainerMemoryStats {
                usage: Some(100_000_000),   // 100 MB
                limit: Some(2_000_000_000), // 2 GB
                max_usage: None,
                stats: None,
                failcnt: None,
                commitbytes: None,
                commitpeakbytes: None,
                privateworkingset: None,
            }),
            ..Default::default()
        };

        assert_eq!(calculate_memory_percentage(&stats), 5.0);
    }

    #[test]
    fn test_calculate_memory_percentage_missing_memory_stats() {
        let stats = ContainerStatsResponse {
            memory_stats: None,
            ..Default::default()
        };

        assert_eq!(calculate_memory_percentage(&stats), 0.0);
    }

    #[test]
    fn test_calculate_memory_percentage_missing_usage() {
        let stats = ContainerStatsResponse {
            memory_stats: Some(ContainerMemoryStats {
                usage: None,
                limit: Some(1_000_000_000),
                max_usage: None,
                stats: None,
                failcnt: None,
                commitbytes: None,
                commitpeakbytes: None,
                privateworkingset: None,
            }),
            ..Default::default()
        };

        assert_eq!(calculate_memory_percentage(&stats), 0.0);
    }

    #[test]
    fn test_calculate_memory_percentage_zero_limit() {
        let stats = ContainerStatsResponse {
            memory_stats: Some(ContainerMemoryStats {
                usage: Some(500_000_000),
                limit: Some(0),
                max_usage: None,
                stats: None,
                failcnt: None,
                commitbytes: None,
                commitpeakbytes: None,
                privateworkingset: None,
            }),
            ..Default::default()
        };

        // Should handle division by zero gracefully
        assert_eq!(calculate_memory_percentage(&stats), 0.0);
    }
}
