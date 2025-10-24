use tokio::sync::mpsc;

/// Container metadata (static information)
#[derive(Clone, Debug)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub status: String,
    pub stats: ContainerStats,
}

/// Container runtime statistics (updated frequently)
#[derive(Clone, Debug, Default)]
pub struct ContainerStats {
    pub cpu: f64,
    pub memory: f64,
}

pub enum AppEvent {
    /// Initial list of containers when app starts
    InitialContainerList(Vec<Container>),
    /// A new container was created/started
    ContainerCreated(Container),
    /// A container was stopped/destroyed
    ContainerDestroyed(String),
    /// Stats update for an existing container
    ContainerStat(String, ContainerStats),
    /// User requested to quit
    Quit,
    /// Terminal was resized
    Resize,
}

pub type EventSender = mpsc::Sender<AppEvent>;
