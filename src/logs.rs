use bollard::query_parameters::LogsOptions;
use futures_util::stream::StreamExt;

use crate::docker::DockerHost;
use crate::types::{AppEvent, ContainerKey, EventSender};

/// Streams logs from a container in real-time
/// Sends each log line as it arrives via the event channel
pub async fn stream_container_logs(host: DockerHost, container_id: String, tx: EventSender) {
    let key = ContainerKey::new(host.host_id.clone(), container_id.clone());

    // Configure log options to stream logs
    let options = Some(LogsOptions {
        follow: true,            // Stream logs in real-time
        stdout: true,            // Include stdout
        stderr: true,            // Include stderr
        tail: "100".to_string(), // Start with last 100 lines
        timestamps: true,        // Include timestamps
        ..Default::default()
    });

    let mut log_stream = host.docker.logs(&container_id, options);

    while let Some(log_result) = log_stream.next().await {
        match log_result {
            Ok(log_output) => {
                // Convert log output to string
                let log_line = log_output.to_string();

                // Send log line event
                if tx
                    .send(AppEvent::LogLine(key.clone(), log_line))
                    .await
                    .is_err()
                {
                    // Channel closed, stop streaming
                    break;
                }
            }
            Err(_) => {
                // Error reading logs, stop streaming
                break;
            }
        }
    }
}
