use crc::{Algorithm, Crc};
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u16)]
pub enum PacketCommand {
    ReadBattery = 49159,
    ReadAnc = 49182,
    SetAnc = 61455,
    ReadEq = 49183,
    SetEq = 61456,
    ReadPersonalizedAnc = 49184,
    SetPersonalizedAnc = 61457,
    ReadListeningMode = 49232,
    SetListeningMode = 61469,
    ReadEnhancedBass = 49230,
    SetEnhancedBass = 61521,
    ReadAdvancedEq = 49228,
    SetAdvancedEqEnabled = 61519,
    ReadCustomEq = 49220,
    SetCustomEq = 61505,
    StartEarFitTest = 61460,
    ReadFirmware = 49218,
    ReadInEar = 49166,
    SetInEar = 61444,
    ReadLatency = 49217,
    SetLatency = 61504,
    SetRingBuds = 61442,
    ReadSku = 49160,
    ReadSkuAlt = 16392,
    RespSku = 57352,
    RespBattery = 57345,
    RespBatteryAlt = 16391,
    RespAnc = 57347,
    RespAncAlt = 16414,
    RespEq = 16415,
    RespEqAlt = 16464,
    RespPersonalizedAnc = 16416,
    RespFirmware = 16450,
    RespInEar = 16398,
    RespLatency = 16449,
    RespGesture = 16408,
    RespEarFitTest = 57357,
    RespEnhancedBass = 16462,
    RespCustomEq = 16452,
    RespAdvancedEqStatus = 16460,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParsedResponse {
    Battery(DeviceBattery),
    Anc(AncMode),
    Eq { mode: EqMode, preset: u8 },
    Firmware(String),
    InEar(bool),
    Latency(bool),
    Gestures(Vec<Gesture>),
    EnhancedBass { enabled: bool, level: u8 },
    CustomEq([f32; 3]),
    EarFitTest { left: u8, right: u8 },
    Sku(String),
    PersonalizedAnc(bool),
    AdvancedEq(bool),
    Unknown(u16, Vec<u8>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BatteryStatus {
    pub level: u8,
    pub is_charging: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceBattery {
    pub left: Option<BatteryStatus>,
    pub right: Option<BatteryStatus>,
    pub case: Option<BatteryStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DeviceId {
    Left = 0x02,
    Right = 0x03,
    Case = 0x04,
}

impl DeviceId {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x02 => Some(Self::Left),
            0x03 => Some(Self::Right),
            0x04 => Some(Self::Case),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum AncMode {
    Off = 0x05,
    Transparent = 0x07,
    NcLow = 0x03,
    NcHigh = 0x01,
    NcMid = 0x02,
    NcAdaptive = 0x04,
}

impl AncMode {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x05 => Some(Self::Off),
            0x07 => Some(Self::Transparent),
            0x03 => Some(Self::NcLow),
            0x01 => Some(Self::NcHigh),
            0x02 => Some(Self::NcMid),
            0x04 => Some(Self::NcAdaptive),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EqMode {
    Balanced = 0,
    MoreTreble = 1,
    MoreBass = 2,
    Voice = 3,
    Custom = 6,
}

impl EqMode {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Balanced),
            1 => Some(Self::MoreTreble),
            2 => Some(Self::MoreBass),
            3 => Some(Self::Voice),
            6 => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum GestureType {
    DoubleTap = 2,
    TripleTap = 3,
    TapAndHold = 7,
    DoubleTapAndHold = 9,
}

impl GestureType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            2 => Some(Self::DoubleTap),
            3 => Some(Self::TripleTap),
            7 => Some(Self::TapAndHold),
            9 => Some(Self::DoubleTapAndHold),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GestureAction {
    NoAction,
    SkipBack,
    SkipForward,
    VoiceAssistant,
    NoiseControl,
    VolumeUp,
    VolumeDown,
    NoiseControlToggles(u8),
    Unknown(u8),
}

impl GestureAction {
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::NoAction,
            8 => Self::SkipBack,
            9 => Self::SkipForward,
            11 => Self::VoiceAssistant,
            10 => Self::NoiseControl,
            18 => Self::VolumeUp,
            19 => Self::VolumeDown,
            20 | 21 | 22 => Self::NoiseControlToggles(value),
            _ => Self::Unknown(value),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gesture {
    pub device: DeviceId,
    pub gesture_type: GestureType,
    pub action: GestureAction,
}

pub fn calculate_crc(data: &[u8]) -> u16 {
    Crc::<u16>::new(&CRC_16_ARC).checksum(data)
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub command: PacketCommand,
    pub payload: Vec<u8>,
    pub operation_id: u8,
}

impl Packet {
    pub fn new(command: PacketCommand, payload: Vec<u8>, operation_id: u8) -> Self {
        Self {
            command,
            payload,
            operation_id,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = vec![
            0x55,
            0x60,
            0x01,
            0,
            0,
            self.payload.len() as u8,
            0,
            self.operation_id,
        ];
        let cmd = (self.command as u16).to_le_bytes();
        data[3] = cmd[0];
        data[4] = cmd[1];
        data.extend_from_slice(&self.payload);
        let crc = calculate_crc(&data);
        data.extend([crc as u8, (crc >> 8) as u8]);
        data
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 10 || bytes[0] != 0x55 {
            return None;
        }

        let command = PacketCommand::try_from(u16::from_le_bytes([bytes[3], bytes[4]])).ok()?;
        let len = bytes[5] as usize;
        let op = bytes[7];

        if bytes.len() < 10 + len {
            return None;
        }

        Some(Self {
            command,
            payload: bytes[8..8 + len].to_vec(),
            operation_id: op,
        })
    }

    pub fn parse(&self) -> ParsedResponse {
        match self.command {
            PacketCommand::RespBattery | PacketCommand::RespBatteryAlt => {
                let mut battery = DeviceBattery::default();
                let count = *self.payload.get(0).unwrap_or(&0) as usize;

                for i in 0..count {
                    let base = 1 + i * 2;
                    if base + 1 >= self.payload.len() {
                        break;
                    }

                    let id = self.payload[base];
                    let s = self.payload[base + 1];
                    let stat = BatteryStatus {
                        level: s & 0x7F,
                        is_charging: s & 0x80 != 0,
                    };

                    match DeviceId::from_u8(id) {
                        Some(DeviceId::Left) => battery.left = Some(stat),
                        Some(DeviceId::Right) => battery.right = Some(stat),
                        Some(DeviceId::Case) => battery.case = Some(stat),
                        _ => {}
                    }
                }

                ParsedResponse::Battery(battery)
            }

            PacketCommand::RespAnc | PacketCommand::RespAncAlt => {
                let v = self.payload.get(1).copied().unwrap_or(0);
                AncMode::from_u8(v)
                    .map(ParsedResponse::Anc)
                    .unwrap_or_else(|| {
                        ParsedResponse::Unknown(self.command as u16, self.payload.clone())
                    })
            }

            PacketCommand::RespEq | PacketCommand::RespEqAlt => {
                let preset = self.payload.get(0).copied().unwrap_or(0);
                let mode = EqMode::from_u8(preset).unwrap_or(EqMode::Balanced);
                ParsedResponse::Eq { mode, preset }
            }

            PacketCommand::RespFirmware => {
                ParsedResponse::Firmware(String::from_utf8_lossy(&self.payload).to_string())
            }

            PacketCommand::RespInEar => {
                ParsedResponse::InEar(self.payload.get(2).copied().unwrap_or(0) != 0)
            }

            PacketCommand::RespLatency => {
                ParsedResponse::Latency(self.payload.get(0).copied().unwrap_or(0) != 0)
            }

            PacketCommand::RespGesture => {
                let count = *self.payload.get(0).unwrap_or(&0) as usize;
                let mut out = Vec::new();

                for i in 0..count {
                    let base = 1 + i * 4;
                    if base + 3 >= self.payload.len() {
                        break;
                    }

                    if let (Some(d), Some(t)) = (
                        DeviceId::from_u8(self.payload[base]),
                        GestureType::from_u8(self.payload[base + 2]),
                    ) {
                        out.push(Gesture {
                            device: d,
                            gesture_type: t,
                            action: GestureAction::from_u8(self.payload[base + 3]),
                        });
                    }
                }

                ParsedResponse::Gestures(out)
            }

            PacketCommand::RespAdvancedEqStatus => {
                ParsedResponse::AdvancedEq(self.payload.get(0) == Some(&1))
            }

            PacketCommand::RespCustomEq | PacketCommand::ReadCustomEq => {
                if self.payload.len() >= 45 {
                    fn from_format_float_for_eq(mut bytes: [u8; 4]) -> f32 {
                        bytes.reverse();

                        if bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 0 && (bytes[3] & 0x80) != 0
                        {
                            bytes[3] &= 0x7f;
                            -f32::from_be_bytes(bytes)
                        } else {
                            f32::from_be_bytes(bytes)
                        }
                    }

                    let mut values = [0.0f32; 3];
                    for (index, slot) in values.iter_mut().enumerate() {
                        let base = 6 + (index * 13);
                        if base + 3 >= self.payload.len() {
                            break;
                        }
                        let bytes = [
                            self.payload[base],
                            self.payload[base + 1],
                            self.payload[base + 2],
                            self.payload[base + 3],
                        ];
                        *slot = from_format_float_for_eq(bytes).round().clamp(-6.0, 6.0);
                    }
                    ParsedResponse::CustomEq([values[2], values[0], values[1]])
                } else {
                    ParsedResponse::Unknown(self.command as u16, self.payload.clone())
                }
            }

            PacketCommand::RespEarFitTest => {
                if self.payload.len() >= 2 {
                    ParsedResponse::EarFitTest {
                        left: self.payload[0],
                        right: self.payload[1],
                    }
                } else {
                    ParsedResponse::Unknown(self.command as u16, self.payload.clone())
                }
            }

            PacketCommand::RespEnhancedBass | PacketCommand::ReadEnhancedBass => {
                if self.payload.len() >= 2 {
                    ParsedResponse::EnhancedBass {
                        enabled: self.payload[0] == 0x01,
                        level: self.payload[1] / 2,
                    }
                } else {
                    ParsedResponse::Unknown(self.command as u16, self.payload.clone())
                }
            }

            PacketCommand::RespPersonalizedAnc | PacketCommand::ReadPersonalizedAnc => {
                ParsedResponse::PersonalizedAnc(self.payload.get(0) == Some(&1))
            }

            PacketCommand::RespSku | PacketCommand::ReadSku | PacketCommand::ReadSkuAlt => {
                let sku = if self.payload.len() >= 2 {
                    format!("{:02x}", self.payload[1])
                } else if !self.payload.is_empty() {
                    format!("{:02x}", self.payload[0])
                } else {
                    "unknown".to_string()
                };
                ParsedResponse::Sku(sku)
            }

            _ => ParsedResponse::Unknown(self.command as u16, self.payload.clone()),
        }
    }
}
