use ratatui::{
    Frame,
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
};
use std::collections::HashMap;

use crate::types::{Container, ContainerKey, ViewState};

/// Pre-allocated styles to avoid recreation every frame
pub struct UiStyles {
    pub high: Style,
    pub medium: Style,
    pub low: Style,
    pub header: Style,
    pub border: Style,
    pub selected: Style,
}

impl Default for UiStyles {
    fn default() -> Self {
        Self {
            high: Style::default().fg(Color::Red),
            medium: Style::default().fg(Color::Yellow),
            low: Style::default().fg(Color::Green),
            header: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            border: Style::default().fg(Color::White),
            selected: Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        }
    }
}

/// Renders the main UI - either container list or log view
pub fn render_ui(
    f: &mut Frame,
    containers: &HashMap<ContainerKey, Container>,
    sorted_container_keys: &[ContainerKey],
    styles: &UiStyles,
    table_state: &mut TableState,
    show_host_column: bool,
    view_state: &ViewState,
    container_logs: &HashMap<ContainerKey, Vec<String>>,
) {
    match view_state {
        ViewState::ContainerList => {
            render_container_list(
                f,
                containers,
                sorted_container_keys,
                styles,
                table_state,
                show_host_column,
            );
        }
        ViewState::LogView(container_key) => {
            render_log_view(f, container_key, containers, container_logs, styles);
        }
    }
}

/// Renders the container list view
fn render_container_list(
    f: &mut Frame,
    containers: &HashMap<ContainerKey, Container>,
    sorted_container_keys: &[ContainerKey],
    styles: &UiStyles,
    table_state: &mut TableState,
    show_host_column: bool,
) {
    let size = f.area();

    // Use pre-sorted list instead of sorting every frame
    let rows: Vec<Row> = sorted_container_keys
        .iter()
        .filter_map(|key| containers.get(key))
        .map(|c| create_container_row(c, styles, show_host_column))
        .collect();

    let header = create_header_row(styles, show_host_column);
    let table = create_table(rows, header, containers.len(), styles, show_host_column);

    f.render_stateful_widget(table, size, table_state);
}

/// Renders the log view for a specific container
fn render_log_view(
    f: &mut Frame,
    container_key: &ContainerKey,
    containers: &HashMap<ContainerKey, Container>,
    container_logs: &HashMap<ContainerKey, Vec<String>>,
    styles: &UiStyles,
) {
    let size = f.area();

    // Get container info
    let container_name = containers
        .get(container_key)
        .map(|c| c.name.as_str())
        .unwrap_or("Unknown");

    // Get logs for this container
    let logs = container_logs
        .get(container_key)
        .map(|l| l.as_slice())
        .unwrap_or(&[]);

    // Join all log lines
    let log_text = logs.join("\n");

    // Create log widget
    let log_widget = Paragraph::new(log_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    "Logs: {} ({}) - Press ESC to return",
                    container_name, container_key.host_id
                ))
                .style(styles.border),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(log_widget, size);
}

/// Creates a table row for a single container
fn create_container_row<'a>(
    container: &'a Container,
    styles: &UiStyles,
    show_host_column: bool,
) -> Row<'a> {
    let cpu_bar = create_progress_bar(container.stats.cpu, 20);
    let cpu_style = get_percentage_style(container.stats.cpu, styles);

    let memory_bar = create_progress_bar(container.stats.memory, 20);
    let memory_style = get_percentage_style(container.stats.memory, styles);

    let network_tx = format_bytes_per_sec(container.stats.network_tx_bytes_per_sec);
    let network_rx = format_bytes_per_sec(container.stats.network_rx_bytes_per_sec);

    let mut cells = vec![
        Cell::from(container.id.as_str()),
        Cell::from(container.name.as_str()),
    ];

    if show_host_column {
        cells.push(Cell::from(container.host_id.as_str()));
    }

    cells.extend(vec![
        Cell::from(cpu_bar).style(cpu_style),
        Cell::from(memory_bar).style(memory_style),
        Cell::from(network_tx),
        Cell::from(network_rx),
        Cell::from(container.status.as_str()),
    ]);

    Row::new(cells)
}

/// Creates a text-based progress bar with percentage
fn create_progress_bar(percentage: f64, width: usize) -> String {
    let percentage = percentage.clamp(0.0, 100.0);
    let filled_width = ((percentage / 100.0) * width as f64).round() as usize;
    let empty_width = width.saturating_sub(filled_width);

    let bar = format!("{}{}", "█".repeat(filled_width), "░".repeat(empty_width));

    format!("{} {:5.1}%", bar, percentage)
}

/// Formats bytes per second into a human-readable string (KB/s, MB/s, GB/s)
fn format_bytes_per_sec(bytes_per_sec: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes_per_sec >= GB {
        format!("{:.2}GB/s", bytes_per_sec / GB)
    } else if bytes_per_sec >= MB {
        format!("{:.2}MB/s", bytes_per_sec / MB)
    } else if bytes_per_sec >= KB {
        format!("{:.1}KB/s", bytes_per_sec / KB)
    } else {
        format!("{:.0}B/s", bytes_per_sec)
    }
}

/// Returns the appropriate style based on percentage value
pub fn get_percentage_style(value: f64, styles: &UiStyles) -> Style {
    if value > 80.0 {
        styles.high
    } else if value > 50.0 {
        styles.medium
    } else {
        styles.low
    }
}

/// Creates the table header row
fn create_header_row(styles: &UiStyles, show_host_column: bool) -> Row<'static> {
    let mut headers = vec!["ID", "Name"];

    if show_host_column {
        headers.push("Host");
    }

    headers.extend(vec!["CPU %", "Memory %", "Net TX", "Net RX", "Status"]);

    Row::new(headers).style(styles.header).bottom_margin(1)
}

/// Creates the complete table widget
fn create_table<'a>(
    rows: Vec<Row<'a>>,
    header: Row<'static>,
    container_count: usize,
    styles: &UiStyles,
    show_host_column: bool,
) -> Table<'a> {
    let mut constraints = vec![
        Constraint::Length(12), // Container ID
        Constraint::Fill(1),    // Name (flexible)
    ];

    if show_host_column {
        constraints.push(Constraint::Length(20)); // Host
    }

    constraints.extend(vec![
        Constraint::Length(28), // CPU progress bar (20 chars + " 100.0%")
        Constraint::Length(28), // Memory progress bar (20 chars + " 100.0%")
        Constraint::Length(12), // Network TX (1.23MB/s)
        Constraint::Length(12), // Network RX (4.56MB/s)
        Constraint::Length(15), // Status
    ]);

    Table::new(rows, constraints)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    "Docker Container CPU Monitor - {} containers (↑/↓ to navigate, 'q' to quit)",
                    container_count
                ))
                .style(styles.border),
        )
        .row_highlight_style(styles.selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ContainerStats;
    use ratatui::{Terminal, backend::TestBackend};

    fn create_test_container(
        host_id: &str,
        id: &str,
        name: &str,
        cpu: f64,
        memory: f64,
    ) -> Container {
        Container {
            id: id.to_string(),
            name: name.to_string(),
            status: "running".to_string(),
            stats: ContainerStats {
                cpu,
                memory,
                network_tx_bytes_per_sec: 0.0,
                network_rx_bytes_per_sec: 0.0,
            },
            host_id: host_id.to_string(),
        }
    }

    #[test]
    fn test_percentage_style_thresholds() {
        let styles = UiStyles::default();

        // Test low threshold (green)
        let low_style = get_percentage_style(30.0, &styles);
        assert_eq!(low_style.fg, Some(Color::Green));

        // Test medium threshold (yellow)
        let medium_style = get_percentage_style(65.0, &styles);
        assert_eq!(medium_style.fg, Some(Color::Yellow));

        // Test high threshold (red)
        let high_style = get_percentage_style(85.0, &styles);
        assert_eq!(high_style.fg, Some(Color::Red));

        // Test boundary cases
        assert_eq!(get_percentage_style(50.0, &styles).fg, Some(Color::Green));
        assert_eq!(get_percentage_style(50.1, &styles).fg, Some(Color::Yellow));
        assert_eq!(get_percentage_style(80.0, &styles).fg, Some(Color::Yellow));
        assert_eq!(get_percentage_style(80.1, &styles).fg, Some(Color::Red));
    }

    #[test]
    fn test_render_empty_container_list() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let containers = HashMap::new();
        let sorted_keys: Vec<ContainerKey> = vec![];
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        let view_state = ViewState::ContainerList;
        let container_logs = HashMap::new();

        terminal
            .draw(|f| {
                render_ui(
                    f,
                    &containers,
                    &sorted_keys,
                    &styles,
                    &mut table_state,
                    true,
                    &view_state,
                    &container_logs,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        // Check that "0 containers" appears in the title
        assert!(content.contains("0 containers"));
    }

    #[test]
    fn test_render_single_container() {
        let backend = TestBackend::new(145, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();
        let key = ContainerKey::new("local".to_string(), "test123".to_string());
        containers.insert(
            key.clone(),
            create_test_container("local", "test123", "nginx", 25.5, 45.0),
        );

        let sorted_keys = vec![key];
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        let view_state = ViewState::ContainerList;
        let container_logs = HashMap::new();

        terminal
            .draw(|f| {
                render_ui(
                    f,
                    &containers,
                    &sorted_keys,
                    &styles,
                    &mut table_state,
                    false,
                    &view_state,
                    &container_logs,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        // Verify container appears in output
        assert!(content.contains("nginx"));
        // Host column should be hidden with single host
        assert!(!content.contains("local"));
        assert!(content.contains("1 containers"));
    }

    #[test]
    fn test_render_container_with_high_cpu() {
        let backend = TestBackend::new(145, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();
        let key = ContainerKey::new("local".to_string(), "test123".to_string());
        containers.insert(
            key.clone(),
            create_test_container("local", "test123", "nginx", 85.5, 45.0),
        );

        let sorted_keys = vec![key];
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        let view_state = ViewState::ContainerList;
        let container_logs = HashMap::new();

        terminal
            .draw(|f| {
                render_ui(
                    f,
                    &containers,
                    &sorted_keys,
                    &styles,
                    &mut table_state,
                    false,
                    &view_state,
                    &container_logs,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        // Verify the CPU percentage appears in the output (now in progress bar format)
        assert!(
            content.contains("85.5%"),
            "Should find CPU value 85.5% in buffer"
        );

        // Check that we have red-colored cells in the high CPU range
        let red_cells: Vec<_> = buffer
            .content()
            .iter()
            .filter(|cell| cell.fg == Color::Red)
            .collect();

        assert!(
            !red_cells.is_empty(),
            "Should have red-colored cells for high CPU"
        );
    }

    #[test]
    fn test_render_multiple_containers_sorted() {
        let backend = TestBackend::new(145, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();

        let key1 = ContainerKey::new("local".to_string(), "1".to_string());
        let key2 = ContainerKey::new("local".to_string(), "2".to_string());
        let key3 = ContainerKey::new("local".to_string(), "3".to_string());

        containers.insert(
            key1.clone(),
            create_test_container("local", "1", "zebra", 10.0, 20.0),
        );
        containers.insert(
            key2.clone(),
            create_test_container("local", "2", "apache", 30.0, 40.0),
        );
        containers.insert(
            key3.clone(),
            create_test_container("local", "3", "mysql", 50.0, 60.0),
        );

        // Sort by name alphabetically
        let sorted_keys = vec![key2, key3, key1];
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        let view_state = ViewState::ContainerList;
        let container_logs = HashMap::new();

        terminal
            .draw(|f| {
                render_ui(
                    f,
                    &containers,
                    &sorted_keys,
                    &styles,
                    &mut table_state,
                    false,
                    &view_state,
                    &container_logs,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        // All containers should appear
        assert!(content.contains("apache"));
        assert!(content.contains("mysql"));
        assert!(content.contains("zebra"));
        assert!(content.contains("3 containers"));

        // Verify sorting by checking position of container names
        let apache_pos = content.find("apache").unwrap();
        let mysql_pos = content.find("mysql").unwrap();
        let zebra_pos = content.find("zebra").unwrap();

        // Should be alphabetically sorted
        assert!(apache_pos < mysql_pos);
        assert!(mysql_pos < zebra_pos);
    }

    #[test]
    fn test_render_multi_host_containers() {
        let backend = TestBackend::new(150, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();

        let key1 = ContainerKey::new("local".to_string(), "c1".to_string());
        let key2 = ContainerKey::new("server1".to_string(), "c2".to_string());
        let key3 = ContainerKey::new("server2".to_string(), "c3".to_string());

        containers.insert(
            key1.clone(),
            create_test_container("local", "c1", "nginx", 10.0, 20.0),
        );
        containers.insert(
            key2.clone(),
            create_test_container("server1", "c2", "postgres", 30.0, 40.0),
        );
        containers.insert(
            key3.clone(),
            create_test_container("server2", "c3", "redis", 50.0, 60.0),
        );

        // Sort by host, then name
        let sorted_keys = vec![key1, key2, key3];
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        let view_state = ViewState::ContainerList;
        let container_logs = HashMap::new();

        terminal
            .draw(|f| {
                render_ui(
                    f,
                    &containers,
                    &sorted_keys,
                    &styles,
                    &mut table_state,
                    true,
                    &view_state,
                    &container_logs,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        // All hosts and containers should appear
        assert!(content.contains("local"));
        assert!(content.contains("server1"));
        assert!(content.contains("server2"));
        assert!(content.contains("nginx"));
        assert!(content.contains("postgres"));
        assert!(content.contains("redis"));
        assert!(content.contains("3 containers"));
    }

    #[test]
    fn test_ui_snapshot_empty() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let containers = HashMap::new();
        let sorted_keys: Vec<ContainerKey> = vec![];
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        let view_state = ViewState::ContainerList;
        let container_logs = HashMap::new();

        terminal
            .draw(|f| {
                render_ui(
                    f,
                    &containers,
                    &sorted_keys,
                    &styles,
                    &mut table_state,
                    true,
                    &view_state,
                    &container_logs,
                )
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        insta::assert_snapshot!("empty_container_list", format!("{:?}", buffer));
    }

    #[test]
    fn test_ui_snapshot_single_container_low_usage() {
        let backend = TestBackend::new(145, 25);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();
        let key = ContainerKey::new("local".to_string(), "abc123def456".to_string());
        containers.insert(
            key.clone(),
            create_test_container("local", "abc123def456", "nginx", 25.5, 30.2),
        );

        let sorted_keys = vec![key];
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        let view_state = ViewState::ContainerList;
        let container_logs = HashMap::new();

        terminal
            .draw(|f| {
                render_ui(
                    f,
                    &containers,
                    &sorted_keys,
                    &styles,
                    &mut table_state,
                    false,
                    &view_state,
                    &container_logs,
                )
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        insta::assert_snapshot!("single_container_low_usage", format!("{:?}", buffer));
    }

    #[test]
    fn test_ui_snapshot_multiple_containers_mixed_usage() {
        let backend = TestBackend::new(160, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();

        let key1 = ContainerKey::new("local".to_string(), "container1".to_string());
        let key2 = ContainerKey::new("local".to_string(), "container2".to_string());
        let key3 = ContainerKey::new("local".to_string(), "container3".to_string());

        containers.insert(
            key1.clone(),
            create_test_container("local", "container1", "nginx", 15.5, 25.0),
        );
        containers.insert(
            key2.clone(),
            create_test_container("local", "container2", "postgres", 65.2, 70.5),
        );
        containers.insert(
            key3.clone(),
            create_test_container("local", "container3", "redis", 92.8, 88.3),
        );

        let sorted_keys = vec![key1, key2, key3];
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        let view_state = ViewState::ContainerList;
        let container_logs = HashMap::new();

        terminal
            .draw(|f| {
                render_ui(
                    f,
                    &containers,
                    &sorted_keys,
                    &styles,
                    &mut table_state,
                    false,
                    &view_state,
                    &container_logs,
                )
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        insta::assert_snapshot!("multiple_containers_mixed_usage", format!("{:?}", buffer));
    }

    #[test]
    fn test_color_coding_boundaries() {
        let styles = UiStyles::default();

        // Test exact boundary values
        assert_eq!(
            get_percentage_style(0.0, &styles).fg,
            Some(Color::Green),
            "0% should be green"
        );
        assert_eq!(
            get_percentage_style(50.0, &styles).fg,
            Some(Color::Green),
            "50% should be green"
        );
        assert_eq!(
            get_percentage_style(50.1, &styles).fg,
            Some(Color::Yellow),
            "50.1% should be yellow"
        );
        assert_eq!(
            get_percentage_style(80.0, &styles).fg,
            Some(Color::Yellow),
            "80% should be yellow"
        );
        assert_eq!(
            get_percentage_style(80.1, &styles).fg,
            Some(Color::Red),
            "80.1% should be red"
        );
        assert_eq!(
            get_percentage_style(100.0, &styles).fg,
            Some(Color::Red),
            "100% should be red"
        );
    }
}
