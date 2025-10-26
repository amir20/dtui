use ratatui::{
    Frame,
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};
use std::collections::HashMap;

use crate::types::Container;

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

/// Renders the main UI table showing container stats
pub fn render_ui(
    f: &mut Frame,
    containers: &HashMap<String, Container>,
    sorted_container_ids: &[String],
    styles: &UiStyles,
    table_state: &mut TableState,
) {
    let size = f.area();

    // Use pre-sorted list instead of sorting every frame
    let rows: Vec<Row> = sorted_container_ids
        .iter()
        .filter_map(|id| containers.get(id))
        .map(|c| create_container_row(c, styles))
        .collect();

    let header = create_header_row(styles);
    let table = create_table(rows, header, containers.len(), styles);

    f.render_stateful_widget(table, size, table_state);
}

/// Creates a table row for a single container
fn create_container_row<'a>(container: &'a Container, styles: &UiStyles) -> Row<'a> {
    let cpu_style = get_percentage_style(container.stats.cpu, styles);
    let memory_style = get_percentage_style(container.stats.memory, styles);

    Row::new(vec![
        Cell::from(container.id.as_str()),
        Cell::from(container.name.as_str()),
        Cell::from(format!("{:.2}%", container.stats.cpu)).style(cpu_style),
        Cell::from(format!("{:.2}%", container.stats.memory)).style(memory_style),
        Cell::from(container.status.as_str()),
    ])
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
fn create_header_row(styles: &UiStyles) -> Row<'static> {
    Row::new(vec!["Container ID", "Name", "CPU %", "Memory %", "Status"])
        .style(styles.header)
        .bottom_margin(1)
}

/// Creates the complete table widget
fn create_table<'a>(
    rows: Vec<Row<'a>>,
    header: Row<'static>,
    container_count: usize,
    styles: &UiStyles,
) -> Table<'a> {
    Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Max(30),
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Length(15),
        ],
    )
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

    fn create_test_container(id: &str, name: &str, cpu: f64, memory: f64) -> Container {
        Container {
            id: id.to_string(),
            name: name.to_string(),
            status: "running".to_string(),
            stats: ContainerStats { cpu, memory },
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
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        terminal
            .draw(|f| {
                render_ui(f, &containers, &styles, &mut table_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        // Check that "0 containers" appears in the title
        assert!(content.contains("0 containers"));
    }

    #[test]
    fn test_render_single_container() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();
        containers.insert(
            "test123".to_string(),
            create_test_container("test123", "nginx", 25.5, 45.0),
        );

        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        terminal
            .draw(|f| {
                render_ui(f, &containers, &styles, &mut table_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        // Verify container appears in output
        assert!(content.contains("nginx"));
        assert!(content.contains("1 containers"));
    }

    #[test]
    fn test_render_container_with_high_cpu() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();
        containers.insert(
            "test123".to_string(),
            create_test_container("test123", "nginx", 85.5, 45.0),
        );

        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        terminal
            .draw(|f| {
                render_ui(f, &containers, &styles, &mut table_state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        // Verify the CPU percentage appears in the output
        assert!(
            content.contains("85.50%"),
            "Should find CPU value 85.50% in buffer"
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
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();
        containers.insert(
            "1".to_string(),
            create_test_container("1", "zebra", 10.0, 20.0),
        );
        containers.insert(
            "2".to_string(),
            create_test_container("2", "apache", 30.0, 40.0),
        );
        containers.insert(
            "3".to_string(),
            create_test_container("3", "mysql", 50.0, 60.0),
        );

        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        terminal
            .draw(|f| {
                render_ui(f, &containers, &styles, &mut table_state);
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
    fn test_ui_snapshot_empty() {
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        let containers = HashMap::new();
        let styles = UiStyles::default();
        let mut table_state = TableState::default();

        terminal
            .draw(|f| render_ui(f, &containers, &styles, &mut table_state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        insta::assert_snapshot!("empty_container_list", format!("{:?}", buffer));
    }

    #[test]
    fn test_ui_snapshot_single_container_low_usage() {
        let backend = TestBackend::new(100, 25);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();
        containers.insert(
            "abc123def456".to_string(),
            create_test_container("abc123def456", "nginx", 25.5, 30.2),
        );

        let styles = UiStyles::default();
        let mut table_state = TableState::default();
        terminal
            .draw(|f| render_ui(f, &containers, &styles, &mut table_state))
            .unwrap();

        let buffer = terminal.backend().buffer();
        insta::assert_snapshot!("single_container_low_usage", format!("{:?}", buffer));
    }

    #[test]
    fn test_ui_snapshot_multiple_containers_mixed_usage() {
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        let mut containers = HashMap::new();
        containers.insert(
            "container1".to_string(),
            create_test_container("container1", "nginx", 15.5, 25.0),
        );
        containers.insert(
            "container2".to_string(),
            create_test_container("container2", "postgres", 65.2, 70.5),
        );
        containers.insert(
            "container3".to_string(),
            create_test_container("container3", "redis", 92.8, 88.3),
        );

        let styles = UiStyles::default();
        let mut table_state = TableState::default();
        terminal
            .draw(|f| render_ui(f, &containers, &styles, &mut table_state))
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
