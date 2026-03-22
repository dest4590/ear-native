use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::protocol::Packet;

use super::BluetoothEvent;

pub type BluetoothError = Box<dyn std::error::Error + Send + Sync>;
pub type BluetoothResult<T> = Result<T, BluetoothError>;

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub paired: bool,
    pub system_connected: bool,
}

#[derive(Debug, Clone)]
pub enum StreamRead {
    Data(Vec<u8>),
    Closed,
}

#[async_trait]
pub trait BluetoothAdapter: Send + Sync {
    async fn start_discovery(&self, tx: mpsc::Sender<BluetoothEvent>) -> BluetoothResult<()>;
    async fn paired_devices(&self) -> BluetoothResult<Vec<DiscoveredDevice>>;
    async fn connect(&self, id: &str) -> BluetoothResult<Box<dyn BluetoothStream>>;
}

#[async_trait]
pub trait BluetoothStream: Send {
    async fn send(&mut self, packet: &Packet) -> BluetoothResult<()>;
    async fn read(&mut self) -> BluetoothResult<StreamRead>;
    async fn close(&mut self) -> BluetoothResult<()>;
}
