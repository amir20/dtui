use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};
use std::collections::HashMap;

use crate::types::ContainerInfo;

/// Pre-allocated styles to avoid recreation every frame
pub struct UiStyles {
    pub high: Style,
    pub medium: Style,
    pub low: Style,
    pub header: Style,
    pub border: Style,
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
        }
    }
}

/// Renders the main UI table showing container stats
pub fn render_ui(f: &mut Frame, containers: &HashMap<String, ContainerInfo>, styles: &UiStyles) {
    let size = f.area();

    // Collect references instead of cloning
    let mut container_refs: Vec<_> = containers.values().collect();
    container_refs.sort_by(|a, b| a.name.cmp(&b.name));

    let rows: Vec<Row> = container_refs
        .iter()
        .map(|c| create_container_row(c, styles))
        .collect();

    let header = create_header_row(styles);
    let table = create_table(rows, header, containers.len(), styles);

    f.render_widget(table, size);
}

/// Creates a table row for a single container
fn create_container_row(container: &ContainerInfo, styles: &UiStyles) -> Row {
    let cpu_style = get_percentage_style(container.cpu, styles);
    let memory_style = get_percentage_style(container.memory, styles);

    Row::new(vec![
        Cell::from(container.id.as_str()),
        Cell::from(container.name.as_str()),
        Cell::from(format!("{:.2}%", container.cpu)).style(cpu_style),
        Cell::from(format!("{:.2}%", container.memory)).style(memory_style),
        Cell::from(container.status.as_str()),
    ])
}

/// Returns the appropriate style based on percentage value
fn get_percentage_style(value: f64, styles: &UiStyles) -> Style {
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
fn create_table(
    rows: Vec<Row>,
    header: Row<'static>,
    container_count: usize,
    styles: &UiStyles,
) -> Table {
    Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Min(20),
            Constraint::Length(12),
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
                container_count
            ))
            .style(styles.border),
    )
}
