use serde::de::{self, MapAccess, Visitor};
use serde::Serialize;
use serde::{Deserialize, Deserializer};
use serde_json::json;
use std::fmt;

impl TryFrom<&str> for Message {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, crate::Error> {
        match serde_json::from_str(value) {
            Ok(m) => Ok(m),
            Err(e) => Err(crate::Error::ParseError(e)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Message {
    Error(DeviceError),
    Heartbeat(Heartbeat),
    Saved(Saved),
    Event(Event),
    Profiles(Profiles),
    Profile(ProfileRoot),
    Settings(SettingsRoot),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Command {
    GetProfiles,
    GetProfile(String),
    SetProfile(String),
    GetSettings,
    Save,
    Load,
    Recalibrate,
    ShowMessage(MessageDetails),
    SetScreen(ScreenData),
}

impl ToString for Command {
    fn to_string(&self) -> String {
        let val = match self {
            Command::GetProfiles => json!({
                "profiles": "#all",
            }),
            Command::GetProfile(profile) => json!({
                "profile": profile,
            }),
            Command::SetProfile(profile) => json!({
                "current": profile,
            }),
            Command::GetSettings => json!({
                "settings": "?",
            }),
            Command::Save => json!({
                "save": true,
            }),
            Command::Load => json!({
                "load": true,
            }),
            Command::Recalibrate => json!({
                "recalibrate": true,
            }),
            Command::ShowMessage(msg) => {
                json!(SetMessage {
                    screen: msg.clone()
                })
            }
            Command::SetScreen(msg) => {
                json!(SetScreen {
                    screen: msg.clone()
                })
            }
        };

        val.to_string()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeviceError {
    pub error: String,
    pub msg: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Heartbeat {
    idle: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Saved {
    saved: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(untagged)]
pub enum Event {
    Position(u64),
    Key(KeyEvent),
}

#[derive(Serialize, Debug, Clone)]
pub enum KeyEvent {
    Down { keys: [bool; 4], id: u8 },
    Up { keys: [bool; 4], id: u8 },
}

struct EventVisitor;

impl<'de> Visitor<'de> for EventVisitor {
    type Value = Event;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a Position or KeyEvent")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut p: Option<u64> = None;
        let mut ks: Option<u32> = None;
        let mut kd: Option<u32> = None;
        let mut ku: Option<u32> = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "p" => {
                    if p.is_some() {
                        return Err(de::Error::duplicate_field("p"));
                    }
                    p = Some(map.next_value()?);
                }
                "ks" => {
                    if ks.is_some() {
                        return Err(de::Error::duplicate_field("ks"));
                    }
                    ks = Some(map.next_value()?);
                }
                "kd" => {
                    if kd.is_some() {
                        return Err(de::Error::duplicate_field("kd"));
                    }
                    kd = Some(map.next_value()?);
                }
                "ku" => {
                    if ku.is_some() {
                        return Err(de::Error::duplicate_field("ku"));
                    }
                    ku = Some(map.next_value()?);
                }
                _ => return Err(de::Error::unknown_field(&key, &["p", "ks", "kd", "ku"])),
            }
        }

        if let Some(pos) = p {
            return Ok(Event::Position(pos));
        }

        let keys = if let Some(ks) = ks {
            [ks & 1 != 0, ks & 2 != 0, ks & 4 != 0, ks & 8 != 0]
        } else {
            [false; 4]
        };

        if let Some(kd) = kd {
            Ok(Event::Key(KeyEvent::Down { keys, id: kd as u8 }))
        } else if let Some(ku) = ku {
            Ok(Event::Key(KeyEvent::Up { keys, id: ku as u8 }))
        } else {
            Err(de::Error::missing_field("kd or ku"))
        }
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(EventVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Profiles {
    pub profiles: Option<Vec<String>>,
    #[serde(rename = "current")]
    pub current_profile: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfileRoot {
    pub profile: Profile,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub version: u8,
    pub name: String,
    pub desc: String,
    pub profile_tag: String,
    pub led_enable: bool,
    pub led_brightness: u8,
    pub led_mode: u8,
    pub pointer: u32,
    pub primary: u32,
    pub secondary: u32,
    pub attract_distance: Option<u32>,
    pub feedback_strength: Option<u32>,
    pub bounce_strength: Option<u32>,
    pub haptic_click_strength: Option<u32>,
    pub button_a_idle: u32,
    pub button_b_idle: u32,
    pub button_c_idle: u32,
    pub button_d_idle: u32,
    pub button_a_press: u32,
    pub button_b_press: u32,
    pub button_c_press: u32,
    pub button_d_press: u32,
    pub keys: Vec<KeyDef>,
    pub knob: Vec<Knob>,
    pub gui_enable: bool,
    pub audio: Audio,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KeyDef {
    pub pressed: Vec<KeyPress>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KeyPress {
    pub r#type: String, // "type" is a reserved keyword in Rust, so we use "r#type"
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Knob {
    pub value_min: u8,
    pub value_max: u8,
    pub angle_min: u8,
    pub angle_max: u8,
    pub wrap: bool,
    pub step: u8,
    pub key_state: u8,
    pub haptic: Haptic,
    pub r#type: String, // "type" is a reserved keyword in Rust, so we use "r#type"
    pub channel: u8,
    pub cc: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Haptic {
    pub mode: u8,
    pub start_pos: u8,
    pub end_pos: u8,
    pub detent_count: u8,
    pub vernier: u8,
    pub kx_force: bool,
    pub output_ramp: u32,
    pub detent_strength: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Audio {
    pub click_type: String,
    pub key_click_type: String,
    pub click_level: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SettingsRoot {
    pub settings: Settings,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub debug: bool,
    pub led_max_brightness: u8,
    pub max_velocity: u8,
    pub max_voltage: u8,
    pub device_orientation: u8,
    pub device_name: String,
    pub wifi_enabled: bool,
    pub serial_number: String,
    pub firmware_version: String,
    pub midi_usb: MidiSettings,
    pub midi2: MidiSettings,
    pub sysex_id: u8,
    pub idle_timeout: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MidiSettings {
    pub r#in: bool, // "in" is a reserved keyword in Rust, so we use "r#in"
    pub out: bool,
    pub thru: bool,
    pub route: bool,
    pub nano: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetMessage {
    pub screen: MessageDetails,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageDetails {
    pub title: Option<String>,
    pub text: Option<String>,
    pub duration: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SetScreen {
    pub screen: ScreenData,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScreenData {
    pub title: Option<String>,
    pub data1: Option<String>,
    pub data2: Option<String>,
    pub data3: Option<String>,
    pub data4: Option<String>,
}
