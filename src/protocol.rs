use crate::models::{
    AncMode, BatteryStatus, DeviceBattery, DeviceId, EqMode, Gesture, GestureAction, GestureType,
};
use crc::{Algorithm, Crc};
use serde::{Deserialize, Serialize};

pub const CRC_16_ARC: Algorithm<u16> = Algorithm {
    width: 16,
    poly: 0x8005,
    init: 0xffff,
    refin: true,
    refout: true,
    xorout: 0x0000,
    check: 0xbb3d,
    residue: 0x0000,
};

pub mod commands {
    pub const READ_BATTERY: u16 = 49159;
    pub const READ_ANC: u16 = 49182;
    pub const SET_ANC: u16 = 61455;
    pub const READ_EQ: u16 = 49183;
    pub const SET_EQ: u16 = 61456;
    pub const READ_PERSONALIZED_ANC: u16 = 49184;
    pub const SET_PERSONALIZED_ANC: u16 = 61457;
    pub const READ_LISTENING_MODE: u16 = 49232;
    pub const SET_LISTENING_MODE: u16 = 61469;
    pub const READ_ENHANCED_BASS: u16 = 49230;
    pub const SET_ENHANCED_BASS: u16 = 61521;
    pub const READ_ADVANCED_EQ: u16 = 49228;
    pub const SET_ADVANCED_EQ_ENABLED: u16 = 61519;
    pub const READ_CUSTOM_EQ: u16 = 49220;
    pub const SET_CUSTOM_EQ: u16 = 61505;
    pub const START_EAR_FIT_TEST: u16 = 61460;
    pub const READ_FIRMWARE: u16 = 49218;
    pub const READ_IN_EAR: u16 = 49166;
    pub const SET_IN_EAR: u16 = 61444;
    pub const READ_LATENCY: u16 = 49217;
    pub const SET_LATENCY: u16 = 61504;
    pub const SET_RING_BUDS: u16 = 61442;
    pub const READ_SKU: u16 = 49160;

    // response commands
    pub const RESP_BATTERY: u16 = 57345;
    pub const RESP_BATTERY_ALT: u16 = 16391;
    pub const RESP_ANC: u16 = 57347;
    pub const RESP_ANC_ALT: u16 = 16414;
    pub const RESP_EQ: u16 = 16415;
    pub const RESP_EQ_ALT: u16 = 16464;
    pub const RESP_PERSONALIZED_ANC: u16 = 16416;
    pub const RESP_FIRMWARE: u16 = 16450;
    pub const RESP_IN_EAR: u16 = 16398;
    pub const RESP_LATENCY: u16 = 16449;
    pub const RESP_GESTURE: u16 = 16408;
    pub const RESP_EAR_FIT_TEST: u16 = 57357;
    pub const RESP_ENHANCED_BASS: u16 = 16462;
    pub const RESP_ADVANCED_EQ_STATUS: u16 = 16460;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParsedResponse {
    Battery(DeviceBattery),
    Anc(AncMode),
    Eq(EqMode),
    Firmware(String),
    InEar(bool),
    Latency(bool),
    Gestures(Vec<Gesture>),
    EnhancedBass { enabled: bool, level: u8 },
    Unknown(u16, Vec<u8>),
}

pub fn calculate_crc(data: &[u8]) -> u16 {
    let crc = Crc::<u16>::new(&CRC_16_ARC);
    crc.checksum(data)
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub command: u16,
    pub payload: Vec<u8>,
    pub operation_id: u8,
}

impl Packet {
    pub fn new(command: u16, payload: Vec<u8>, operation_id: u8) -> Self {
        Self {
            command,
            payload,
            operation_id,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut header = vec![0x55, 0x60, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
        let command_bytes = self.command.to_le_bytes();
        header[3] = command_bytes[0];
        header[4] = command_bytes[1];
        header[5] = self.payload.len() as u8;
        header[7] = self.operation_id;

        let mut data = header;
        data.extend_from_slice(&self.payload);

        let crc = calculate_crc(&data);
        data.push((crc & 0xFF) as u8);
        data.push((crc >> 8) as u8);

        data
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 10 || bytes[0] != 0x55 {
            return None;
        }

        let command = u16::from_le_bytes([bytes[3], bytes[4]]);
        let payload_len = bytes[5] as usize;
        let operation_id = bytes[7];

        if bytes.len() < 10 + payload_len {
            return None;
        }

        let payload = bytes[8..8 + payload_len].to_vec();

        Some(Packet {
            command,
            payload,
            operation_id,
        })
    }

    pub fn parse(&self) -> ParsedResponse {
        match self.command {
            commands::RESP_BATTERY | commands::RESP_BATTERY_ALT => {
                // battery response
                // byte 0: connected devices count
                // following bytes: device_id, status_byte (battery | charging_mask)
                if self.payload.len() < 1 {
                    return ParsedResponse::Unknown(self.command, self.payload.clone());
                }
                let count = self.payload[0] as usize;
                let mut battery = DeviceBattery::default();
                for i in 0..count {
                    if self.payload.len() < 1 + (i * 2) + 2 {
                        break;
                    }
                    let device_id = self.payload[1 + (i * 2)];
                    let status = self.payload[2 + (i * 2)];
                    let level = status & 0x7F;
                    let is_charging = (status & 0x80) != 0;
                    let stat = BatteryStatus { level, is_charging };

                    match DeviceId::from_u8(device_id) {
                        Some(DeviceId::Left) => battery.left = Some(stat),
                        Some(DeviceId::Right) => battery.right = Some(stat),
                        Some(DeviceId::Case) => battery.case = Some(stat),
                        None => {}
                    }
                }
                ParsedResponse::Battery(battery)
            }
            commands::RESP_ANC | commands::RESP_ANC_ALT => {
                // ANC response
                // byte 1: anc status
                if self.payload.len() < 2 {
                    return ParsedResponse::Unknown(self.command, self.payload.clone());
                }
                let status = self.payload[1];
                if let Some(mode) = AncMode::from_u8(status) {
                    ParsedResponse::Anc(mode)
                } else {
                    ParsedResponse::Unknown(self.command, self.payload.clone())
                }
            }
            commands::RESP_EQ | commands::RESP_EQ_ALT => {
                // EQ response
                // byte 0: eq mode
                if self.payload.is_empty() {
                    return ParsedResponse::Unknown(self.command, self.payload.clone());
                }
                let mode = self.payload[0];
                if let Some(eq) = EqMode::from_u8(mode) {
                    ParsedResponse::Eq(eq)
                } else {
                    ParsedResponse::Unknown(self.command, self.payload.clone())
                }
            }
            commands::RESP_FIRMWARE => {
                // Firmware response
                // payload contains the version string
                let version = String::from_utf8_lossy(&self.payload).to_string();
                ParsedResponse::Firmware(version)
            }
            commands::RESP_IN_EAR => {
                // In-ear response
                // byte 2: status
                if self.payload.len() < 3 {
                    return ParsedResponse::Unknown(self.command, self.payload.clone());
                }
                ParsedResponse::InEar(self.payload[2] != 0)
            }
            commands::RESP_LATENCY => {
                // Latency response
                // byte 0: status
                if self.payload.is_empty() {
                    return ParsedResponse::Unknown(self.command, self.payload.clone());
                }
                ParsedResponse::Latency(self.payload[0] != 0)
            }
            commands::RESP_GESTURE => {
                // Gesture response
                // byte 0: count
                // following: 4 bytes per gesture (device, common, type, action)
                if self.payload.is_empty() {
                    return ParsedResponse::Unknown(self.command, self.payload.clone());
                }
                let count = self.payload[0] as usize;
                let mut gestures = Vec::new();
                for i in 0..count {
                    if self.payload.len() < 1 + (i * 4) + 4 {
                        break;
                    }
                    let device_id = self.payload[1 + (i * 4)];
                    let _common = self.payload[2 + (i * 4)];
                    let g_type = self.payload[3 + (i * 4)];
                    let action = self.payload[4 + (i * 4)];

                    if let (Some(device), Some(gesture_type)) =
                        (DeviceId::from_u8(device_id), GestureType::from_u8(g_type))
                    {
                        gestures.push(Gesture {
                            device,
                            gesture_type,
                            action: GestureAction::from_u8(action),
                        });
                    }
                }
                ParsedResponse::Gestures(gestures)
            }
            commands::RESP_ADVANCED_EQ_STATUS => {
                // Advanced EQ status response
                // byte 0: status (1 = enabled)
                if self.payload.is_empty() {
                    return ParsedResponse::Unknown(self.command, self.payload.clone());
                }
                if self.payload[0] == 1 {
                    ParsedResponse::Eq(EqMode::Custom)
                } else {
                    ParsedResponse::Unknown(self.command, self.payload.clone())
                }
            }
            _ => ParsedResponse::Unknown(self.command, self.payload.clone()),
        }
    }
}
