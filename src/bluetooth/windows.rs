use async_trait::async_trait;
use tokio::sync::mpsc;
use windows::{
    core::HSTRING,
    Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice},
    Devices::Enumeration::DeviceInformation,
    Networking::Sockets::StreamSocket,
    Storage::Streams::{DataReader, DataWriter, InputStreamOptions},
};

use crate::protocol::Packet;

use super::{
    BluetoothAdapter, BluetoothEvent, BluetoothResult, BluetoothStream, DiscoveredDevice,
    StreamRead,
};

pub async fn create_adapter() -> BluetoothResult<Box<dyn BluetoothAdapter>> {
    Ok(Box::new(WindowsBluetoothAdapter))
}

pub struct WindowsBluetoothAdapter;

#[async_trait]
impl BluetoothAdapter for WindowsBluetoothAdapter {
    async fn start_discovery(&self, tx: mpsc::Sender<BluetoothEvent>) -> BluetoothResult<()> {
        tokio::spawn(async move {
            if let Ok(devices) = query_paired_devices().await {
                for device in devices {
                    let _ = tx.send(BluetoothEvent::DeviceDiscovered(device)).await;
                }
            }
        });

        Ok(())
    }

    async fn paired_devices(&self) -> BluetoothResult<Vec<DiscoveredDevice>> {
        query_paired_devices().await
    }

    async fn connect(&self, id: &str) -> BluetoothResult<Box<dyn BluetoothStream>> {
        let device_id = HSTRING::from(id);
        let device = BluetoothDevice::FromIdAsync(&device_id)?.await?;
        let rfcomm_result = device.GetRfcommServicesAsync()?.await?;

        let service = {
            let services = rfcomm_result.Services()?;
            let service_count = services.Size()?;
            if service_count == 0 {
                return Err("No RFCOMM services found".into());
            }

            let mut spp_service = None;
            let mut fallback_service = None;

            for index in 0..service_count {
                if let Ok(service) = services.GetAt(index) {
                    if let Ok(id) = service.ServiceId() {
                        let uuid = id
                            .Uuid()
                            .map(|value| format!("{:?}", value))
                            .unwrap_or_default()
                            .to_lowercase();

                        if uuid.contains("aeac") {
                            spp_service = Some(service.clone());
                            break;
                        }

                        if uuid.contains("1101") {
                            fallback_service = Some(service.clone());
                        }
                    }
                }
            }

            spp_service
                .or(fallback_service)
                .unwrap_or_else(|| services.GetAt(0).expect("rfcomm service at index 0"))
        };

        let hostname = service.ConnectionHostName()?;
        let service_name = service.ConnectionServiceName()?;
        let socket = StreamSocket::new()?;
        socket.ConnectAsync(&hostname, &service_name)?.await?;

        let output_stream = socket.OutputStream()?;
        let input_stream = socket.InputStream()?;
        let writer = DataWriter::CreateDataWriter(&output_stream)?;
        let reader = DataReader::CreateDataReader(&input_stream)?;
        let _ = reader.SetInputStreamOptions(InputStreamOptions::Partial);

        let read_rx = spawn_reader(reader);

        Ok(Box::new(WindowsBluetoothStream {
            socket,
            writer,
            read_rx,
        }))
    }
}

fn spawn_reader(reader: DataReader) -> WindowsReadChannel {
    let (tx, rx) = mpsc::channel(100);

    let handle = tokio::spawn(async move {
        loop {
            match reader.LoadAsync(4096) {
                Ok(operation) => match operation.await {
                    Ok(loaded) if loaded > 0 => {
                        let mut buffer = vec![0u8; loaded as usize];
                        if reader.ReadBytes(&mut buffer).is_ok() {
                            if tx.send(WindowsReadEvent::Data(buffer)).await.is_err() {
                                break;
                            }
                        } else {
                            let _ = tx.send(WindowsReadEvent::Closed).await;
                            break;
                        }
                    }
                    Ok(_) | Err(_) => {
                        log::info!("Windows BT stream closed from reader thread");
                        let _ = tx.send(WindowsReadEvent::Closed).await;
                        break;
                    }
                },
                Err(_) => {
                    let _ = tx.send(WindowsReadEvent::Closed).await;
                    break;
                }
            }
        }
    });

    WindowsReadChannel {
        rx,
        abort: handle.abort_handle(),
    }
}

struct WindowsReadChannel {
    rx: mpsc::Receiver<WindowsReadEvent>,
    abort: tokio::task::AbortHandle,
}

enum WindowsReadEvent {
    Data(Vec<u8>),
    Closed,
}

struct WindowsBluetoothStream {
    socket: StreamSocket,
    writer: DataWriter,
    read_rx: WindowsReadChannel,
}

#[async_trait]
impl BluetoothStream for WindowsBluetoothStream {
    async fn send(&mut self, packet: &Packet) -> BluetoothResult<()> {
        let bytes = packet.to_bytes();
        if self.writer.WriteBytes(&bytes).is_err() {
            return Err("Write failed".into());
        }

        match self.writer.StoreAsync() {
            Ok(operation) => match operation.await {
                Ok(_) => Ok(()),
                Err(_) => Err("Write failed".into()),
            },
            Err(_) => Err("Write failed".into()),
        }
    }

    async fn read(&mut self) -> BluetoothResult<StreamRead> {
        match self.read_rx.rx.recv().await {
            Some(WindowsReadEvent::Data(data)) => Ok(StreamRead::Data(data)),
            Some(WindowsReadEvent::Closed) | None => Ok(StreamRead::Closed),
        }
    }

    async fn close(&mut self) -> BluetoothResult<()> {
        self.read_rx.abort.abort();
        let _ = self.socket.Close();
        Ok(())
    }
}

async fn query_paired_devices() -> BluetoothResult<Vec<DiscoveredDevice>> {
    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true)
        .map_err(|error| format!("Failed to get device selector: {:?}", error))?;
    let operation = DeviceInformation::FindAllAsyncAqsFilter(&selector)
        .map_err(|error| format!("Failed to enumerate paired devices: {:?}", error))?;
    let devices = operation
        .await
        .map_err(|error| format!("Failed to enumerate paired devices: {:?}", error))?;

    let mut output = Vec::new();
    let size = devices.Size().unwrap_or(0);

    for index in 0..size {
        if let Ok(device_info) = devices.GetAt(index) {
            let id = device_info
                .Id()
                .map(|value: HSTRING| value.to_string())
                .unwrap_or_default();
            let name = device_info
                .Name()
                .map(|value: HSTRING| value.to_string())
                .unwrap_or_default();

            let system_connected = if let Ok(device) =
                BluetoothDevice::FromIdAsync(&HSTRING::from(&id))
            {
                match device.await {
                    Ok(device) => {
                        device.ConnectionStatus().ok() == Some(BluetoothConnectionStatus::Connected)
                    }
                    Err(_) => false,
                }
            } else {
                false
            };

            output.push(DiscoveredDevice {
                id,
                name,
                paired: true,
                system_connected,
            });
        }
    }

    Ok(output)
}
