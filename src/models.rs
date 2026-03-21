use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub base: String,
    pub left_img: String,
    pub case_img: String,
    pub right_img: String,
    pub duo_img: String,
    pub is_anc: bool,
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
    Custom = 6, // Advanced EQ
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
    NoiseControlToggles(u8), // 20, 21, 22
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

    pub fn to_u8(&self) -> u8 {
        match self {
            Self::NoAction => 1,
            Self::SkipBack => 8,
            Self::SkipForward => 9,
            Self::VoiceAssistant => 11,
            Self::NoiseControl => 10,
            Self::VolumeUp => 18,
            Self::VolumeDown => 19,
            Self::NoiseControlToggles(v) => *v,
            Self::Unknown(v) => *v,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gesture {
    pub device: DeviceId,
    pub gesture_type: GestureType,
    pub action: GestureAction,
}

pub fn get_models() -> HashMap<String, ModelInfo> {
    let mut m = HashMap::new();
    m.insert(
        "ear_1_white".to_string(),
        ModelInfo {
            name: "Nothing Ear (1)".to_string(),
            base: "B181".to_string(),
            left_img: "res/assets/ear_one_white_left.webp".to_string(),
            case_img: "res/assets/ear_one_white_case.webp".to_string(),
            right_img: "res/assets/ear_one_white_right.webp".to_string(),
            duo_img: "res/assets/ear_one_white_duo.webp".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "ear_1_black".to_string(),
        ModelInfo {
            name: "Nothing Ear (1)".to_string(),
            base: "B181".to_string(),
            left_img: "res/assets/ear_one_black_left.webp".to_string(),
            case_img: "res/assets/ear_one_black_case.webp".to_string(),
            right_img: "res/assets/ear_one_black_right.webp".to_string(),
            duo_img: "res/assets/ear_one_black_duo.webp".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "ear_stick".to_string(),
        ModelInfo {
            name: "Nothing Ear (stick)".to_string(),
            base: "B157".to_string(),
            left_img: "res/assets/ear_stick_left.webp".to_string(),
            case_img: "res/assets/ear_stick_case_none.webp".to_string(),
            right_img: "res/assets/ear_stick_right.webp".to_string(),
            duo_img: "res/assets/ear_stick_white_duo.webp".to_string(),
            is_anc: false,
        },
    );
    m.insert(
        "ear_2_white".to_string(),
        ModelInfo {
            name: "Nothing Ear (2)".to_string(),
            base: "B155".to_string(),
            left_img: "res/assets/ear_two_white_left.webp".to_string(),
            case_img: "res/assets/ear_two_white_case.webp".to_string(),
            right_img: "res/assets/ear_two_white_right.webp".to_string(),
            duo_img: "res/assets/ear_two_white_duo.webp".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "ear_2_black".to_string(),
        ModelInfo {
            name: "Nothing Ear (2)".to_string(),
            base: "B155".to_string(),
            left_img: "res/assets/ear_two_black_left.webp".to_string(),
            case_img: "res/assets/ear_two_black_case.webp".to_string(),
            right_img: "res/assets/ear_two_black_right.webp".to_string(),
            duo_img: "res/assets/ear_two_black_duo.webp".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "corsola_orange".to_string(),
        ModelInfo {
            name: "CMF Buds Pro".to_string(),
            base: "B163".to_string(),
            left_img: "res/assets/ear_corsola_orange_left.webp".to_string(),
            case_img: "res/assets/ear_corsola_orange_case.webp".to_string(),
            right_img: "res/assets/ear_corsola_orange_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "corsola_black".to_string(),
        ModelInfo {
            name: "CMF Buds Pro".to_string(),
            base: "B163".to_string(),
            left_img: "res/assets/ear_corsola_black_left.webp".to_string(),
            case_img: "res/assets/ear_corsola_black_case.webp".to_string(),
            right_img: "res/assets/ear_corsola_black_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "corsola_white".to_string(),
        ModelInfo {
            name: "CMF Buds Pro".to_string(),
            base: "B163".to_string(),
            left_img: "res/assets/ear_corsola_white_left.webp".to_string(),
            case_img: "res/assets/ear_corsola_white_case.webp".to_string(),
            right_img: "res/assets/ear_corsola_white_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "entei_black".to_string(),
        ModelInfo {
            name: "Nothing Ear".to_string(),
            base: "B171".to_string(),
            left_img: "res/assets/ear_twos_black_left.webp".to_string(),
            case_img: "res/assets/ear_twos_black_case.webp".to_string(),
            right_img: "res/assets/ear_twos_black_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "entei_white".to_string(),
        ModelInfo {
            name: "Nothing Ear".to_string(),
            base: "B171".to_string(),
            left_img: "res/assets/ear_twos_white_left.webp".to_string(),
            case_img: "res/assets/ear_twos_white_case.webp".to_string(),
            right_img: "res/assets/ear_twos_white_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "cleffa_black".to_string(),
        ModelInfo {
            name: "Nothing Ear (a)".to_string(),
            base: "B162".to_string(),
            left_img: "res/assets/ear_color_black_left.webp".to_string(),
            case_img: "res/assets/ear_color_black_case.webp".to_string(),
            right_img: "res/assets/ear_color_black_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "cleffa_white".to_string(),
        ModelInfo {
            name: "Nothing Ear (a)".to_string(),
            base: "B162".to_string(),
            left_img: "res/assets/ear_color_white_left.webp".to_string(),
            case_img: "res/assets/ear_color_white_case.webp".to_string(),
            right_img: "res/assets/ear_color_white_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "cleffa_yellow".to_string(),
        ModelInfo {
            name: "Nothing Ear (a)".to_string(),
            base: "B162".to_string(),
            left_img: "res/assets/ear_color_yellow_left.webp".to_string(),
            case_img: "res/assets/ear_color_yellow_case.webp".to_string(),
            right_img: "res/assets/ear_color_yellow_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "crobat_orange".to_string(),
        ModelInfo {
            name: "CMF Neckband Pro".to_string(),
            base: "B164".to_string(),
            left_img: "".to_string(),
            case_img: "".to_string(),
            right_img: "".to_string(),
            duo_img: "res/assets/crobat_orange.webp".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "crobat_white".to_string(),
        ModelInfo {
            name: "CMF Neckband Pro".to_string(),
            base: "B164".to_string(),
            left_img: "".to_string(),
            case_img: "".to_string(),
            right_img: "".to_string(),
            duo_img: "res/assets/crobat_white.webp".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "crobat_black".to_string(),
        ModelInfo {
            name: "CMF Neckband Pro".to_string(),
            base: "B164".to_string(),
            left_img: "".to_string(),
            case_img: "".to_string(),
            right_img: "".to_string(),
            duo_img: "res/assets/crobat_black.webp".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "donphan_black".to_string(),
        ModelInfo {
            name: "CMF Buds".to_string(),
            base: "B168".to_string(),
            left_img: "res/assets/donphan_black_left.webp".to_string(),
            case_img: "res/assets/donphan_black_case.webp".to_string(),
            right_img: "res/assets/donphan_black_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "donphan_white".to_string(),
        ModelInfo {
            name: "CMF Buds".to_string(),
            base: "B168".to_string(),
            left_img: "res/assets/donphan_white_left.webp".to_string(),
            case_img: "res/assets/donphan_white_case.webp".to_string(),
            right_img: "res/assets/donphan_white_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "donphan_orange".to_string(),
        ModelInfo {
            name: "CMF Buds".to_string(),
            base: "B168".to_string(),
            left_img: "res/assets/donphan_orange_left.webp".to_string(),
            case_img: "res/assets/donphan_orange_case.webp".to_string(),
            right_img: "res/assets/donphan_orange_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "espeon_black".to_string(),
        ModelInfo {
            name: "CMF Buds Pro 2".to_string(),
            base: "B172".to_string(),
            left_img: "res/assets/espeon_black_left.webp".to_string(),
            case_img: "res/assets/espeon_black_case.webp".to_string(),
            right_img: "res/assets/espeon_black_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "espeon_white".to_string(),
        ModelInfo {
            name: "CMF Buds Pro 2".to_string(),
            base: "B172".to_string(),
            left_img: "res/assets/espeon_white_left.webp".to_string(),
            case_img: "res/assets/espeon_white_case.webp".to_string(),
            right_img: "res/assets/espeon_white_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "espeon_orange".to_string(),
        ModelInfo {
            name: "CMF Buds Pro 2".to_string(),
            base: "B172".to_string(),
            left_img: "res/assets/espeon_orange_left.webp".to_string(),
            case_img: "res/assets/espeon_orange_case.webp".to_string(),
            right_img: "res/assets/espeon_orange_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "espeon_blue".to_string(),
        ModelInfo {
            name: "CMF Buds Pro 2".to_string(),
            base: "B172".to_string(),
            left_img: "res/assets/espeon_blue_left.webp".to_string(),
            case_img: "res/assets/espeon_blue_case.webp".to_string(),
            right_img: "res/assets/espeon_blue_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: true,
        },
    );
    m.insert(
        "flaaffy_white".to_string(),
        ModelInfo {
            name: "Nothing Ear (open)".to_string(),
            base: "B174".to_string(),
            left_img: "res/assets/flaffy_white_left.webp".to_string(),
            case_img: "res/assets/flaffy_white_case.webp".to_string(),
            right_img: "res/assets/flaffy_white_right.webp".to_string(),
            duo_img: "".to_string(),
            is_anc: false,
        },
    );

    m
}

pub fn get_sku_map() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("01".to_string(), "ear_1_white".to_string());
    m.insert("02".to_string(), "ear_1_black".to_string());
    m.insert("03".to_string(), "ear_1_white".to_string());
    m.insert("04".to_string(), "ear_1_black".to_string());
    m.insert("06".to_string(), "ear_1_black".to_string());
    m.insert("07".to_string(), "ear_1_white".to_string());
    m.insert("08".to_string(), "ear_1_black".to_string());
    m.insert("10".to_string(), "ear_1_black".to_string());
    m.insert("14".to_string(), "ear_stick".to_string());
    m.insert("15".to_string(), "ear_stick".to_string());
    m.insert("16".to_string(), "ear_stick".to_string());
    m.insert("17".to_string(), "ear_2_white".to_string());
    m.insert("18".to_string(), "ear_2_white".to_string());
    m.insert("19".to_string(), "ear_2_white".to_string());
    m.insert("27".to_string(), "ear_2_black".to_string());
    m.insert("28".to_string(), "ear_2_black".to_string());
    m.insert("29".to_string(), "ear_2_black".to_string());
    m.insert("30".to_string(), "corsola_black".to_string());
    m.insert("31".to_string(), "corsola_black".to_string());
    m.insert("32".to_string(), "corsola_white".to_string());
    m.insert("33".to_string(), "corsola_white".to_string());
    m.insert("34".to_string(), "corsola_orange".to_string());
    m.insert("35".to_string(), "corsola_orange".to_string());
    m.insert("48".to_string(), "crobat_orange".to_string());
    m.insert("49".to_string(), "crobat_white".to_string());
    m.insert("50".to_string(), "crobat_black".to_string());
    m.insert("51".to_string(), "crobat_black".to_string());
    m.insert("52".to_string(), "crobat_white".to_string());
    m.insert("53".to_string(), "crobat_orange".to_string());
    m.insert("54".to_string(), "donphan_black".to_string());
    m.insert("55".to_string(), "donphan_black".to_string());
    m.insert("56".to_string(), "donphan_white".to_string());
    m.insert("57".to_string(), "donphan_white".to_string());
    m.insert("58".to_string(), "donphan_orange".to_string());
    m.insert("59".to_string(), "donphan_orange".to_string());
    m.insert("61".to_string(), "entei_black".to_string());
    m.insert("62".to_string(), "entei_white".to_string());
    m.insert("63".to_string(), "cleffa_black".to_string());
    m.insert("64".to_string(), "cleffa_white".to_string());
    m.insert("65".to_string(), "cleffa_yellow".to_string());
    m.insert("66".to_string(), "cleffa_black".to_string());
    m.insert("67".to_string(), "cleffa_white".to_string());
    m.insert("68".to_string(), "cleffa_yellow".to_string());
    m.insert("69".to_string(), "entei_black".to_string());
    m.insert("70".to_string(), "entei_white".to_string());
    m.insert("71".to_string(), "cleffa_black".to_string());
    m.insert("72".to_string(), "cleffa_white".to_string());
    m.insert("73".to_string(), "cleffa_yellow".to_string());
    m.insert("74".to_string(), "entei_black".to_string());
    m.insert("75".to_string(), "entei_white".to_string());
    m.insert("76".to_string(), "espeon_black".to_string());
    m.insert("77".to_string(), "espeon_white".to_string());
    m.insert("78".to_string(), "espeon_orange".to_string());
    m.insert("79".to_string(), "espeon_blue".to_string());
    m.insert("80".to_string(), "espeon_blue".to_string());
    m.insert("81".to_string(), "espeon_orange".to_string());
    m.insert("82".to_string(), "espeon_white".to_string());
    m.insert("83".to_string(), "espeon_black".to_string());
    m.insert("05".to_string(), "espeon_white".to_string());
    m.insert("11200005".to_string(), "flaaffy_white".to_string());
    m
}
