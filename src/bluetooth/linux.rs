use async_trait::async_trait;
use bluer::{rfcomm, Adapter, AdapterEvent, Address};
use futures::{pin_mut, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::protocol::Packet;

use super::{
    BluetoothAdapter, BluetoothEvent, BluetoothResult, BluetoothStream, DiscoveredDevice,
    StreamRead,
};

pub async fn create_adapter() -> BluetoothResult<Box<dyn BluetoothAdapter>> {
    let session = bluer::Session::new().await?;
    let adapter = session.default_adapter().await?;
    adapter.set_powered(true).await?;

    Ok(Box::new(LinuxBluetoothAdapter { adapter }))
}

pub struct LinuxBluetoothAdapter {
    adapter: Adapter,
}

#[async_trait]
impl BluetoothAdapter for LinuxBluetoothAdapter {
    async fn start_discovery(&self, tx: mpsc::Sender<BluetoothEvent>) -> BluetoothResult<()> {
        self.adapter.set_powered(true).await?;

        for addr in self.adapter.device_addresses().await? {
            if let Ok(device) = self.adapter.device(addr) {
                let name = device.name().await?.unwrap_or_else(|| addr.to_string());
                let _ = tx
                    .send(BluetoothEvent::DeviceDiscovered(DiscoveredDevice {
                        id: addr.to_string(),
                        name,
                        paired: true,
                        system_connected: device.is_connected().await.unwrap_or(false),
                    }))
                    .await;
            }
        }

        let events = self.adapter.discover_devices().await?;
        let adapter = self.adapter.clone();

        tokio::spawn(async move {
            pin_mut!(events);
            while let Some(event) = events.next().await {
                if let AdapterEvent::DeviceAdded(addr) = event {
                    if let Ok(device) = adapter.device(addr) {
                        let mut name = addr.to_string();
                        if let Ok(Some(device_name)) = device.name().await {
                            name = device_name;
                        }

                        let _ = tx
                            .send(BluetoothEvent::DeviceDiscovered(DiscoveredDevice {
                                id: addr.to_string(),
                                name,
                                paired: true,
                                system_connected: device.is_connected().await.unwrap_or(false),
                            }))
                            .await;
                    }
                }
            }
        });

        Ok(())
    }

    async fn paired_devices(&self) -> BluetoothResult<Vec<DiscoveredDevice>> {
        self.adapter.set_powered(true).await?;

        let mut devices = Vec::new();

        for addr in self.adapter.device_addresses().await? {
            if let Ok(device) = self.adapter.device(addr) {
                devices.push(DiscoveredDevice {
                    id: addr.to_string(),
                    name: device.name().await?.unwrap_or_else(|| addr.to_string()),
                    paired: true,
                    system_connected: device.is_connected().await.unwrap_or(false),
                });
            }
        }

        Ok(devices)
    }

    async fn connect(&self, id: &str) -> BluetoothResult<Box<dyn BluetoothStream>> {
        self.adapter.set_powered(true).await?;
        let addr: Address = id.parse()?;
        let device = self.adapter.device(addr)?;

        if !device.is_connected().await.unwrap_or(false) {
            let _ = device.connect().await;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        for channel in 1..=30 {
            if let Ok(socket) = rfcomm::Socket::new() {
                if let Ok(stream) = socket.connect(rfcomm::SocketAddr::new(addr, channel)).await {
                    return Ok(Box::new(LinuxBluetoothStream { stream }));
                }
            }
        }

        Err("Failed to connect to any RFCOMM channel".into())
    }
}

struct LinuxBluetoothStream {
    stream: rfcomm::Stream,
}

#[async_trait]
impl BluetoothStream for LinuxBluetoothStream {
    async fn send(&mut self, packet: &Packet) -> BluetoothResult<()> {
        self.stream.write_all(&packet.to_bytes()).await?;
        Ok(())
    }

    async fn read(&mut self) -> BluetoothResult<StreamRead> {
        let mut buffer = [0u8; 4096];
        let read = self.stream.read(&mut buffer).await?;
        if read == 0 {
            return Ok(StreamRead::Closed);
        }

        Ok(StreamRead::Data(buffer[..read].to_vec()))
    }

    async fn close(&mut self) -> BluetoothResult<()> {
        self.stream.shutdown().await?;
        Ok(())
    }
}
