use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub cpu: f64,
    pub memory: f64,
    pub status: String,
}

pub enum AppEvent {
    ContainerUpdate(String, ContainerInfo),
    ContainerRemoved(String),
    InitialContainerList(Vec<(String, ContainerInfo)>),
    Quit,
}

pub type EventSender = mpsc::Sender<AppEvent>;
pub type EventReceiver = mpsc::Receiver<AppEvent>;
