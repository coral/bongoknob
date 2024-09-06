use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use std::fmt;

impl TryFrom<&str> for Message {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, crate::Error> {
        match serde_json::from_str(value) {
            Ok(m) => Ok(m),
            Err(e) => {
                println!("Parse error: {:?}", e);
                Err(crate::Error::ParseError(e))
            }
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

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Message::Error(e) => write!(f, "Error: {:?}", e),
            Message::Heartbeat(h) => write!(f, "Heartbeat: {:?}", h),
            Message::Saved(s) => write!(f, "Saved: {:?}", s),
            Message::Event(e) => write!(f, "Event: {}", e),
            Message::Profiles(p) => write!(f, "Profiles: {:?}", p),
            Message::Profile(pr) => write!(f, "Profile: {:?}", pr),
            Message::Settings(s) => write!(f, "Settings: {:?}", s),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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
    SetSettings(Settings),
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
            Command::SetSettings(settings) => {
                json!(SettingsRoot {
                    settings: settings.clone()
                })
            }
        };

        dbg!(&val.to_string());

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

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Event::Position(pos) => write!(f, "Position: {}", pos),
            Event::Key(key) => write!(f, "{}", key),
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub enum KeyEvent {
    Down { keys: [bool; 4], id: u8 },
    Up { keys: [bool; 4], id: u8 },
}

impl fmt::Display for KeyEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KeyEvent::Down { keys: _, id } => {
                write!(f, "[DOWN] Key: {}, State: {}", id, self.pretty_print())
            }
            KeyEvent::Up { keys: _, id } => {
                write!(f, "[UP] Key: {}, State: {}", id, self.pretty_print())
            }
        }
    }
}

impl KeyEvent {
    fn pretty_print(&self) -> String {
        let (keys, _) = match self {
            KeyEvent::Down { keys, id } | KeyEvent::Up { keys, id } => (keys, id),
        };

        let keys_str: String = keys
            .iter()
            .map(|&key| if key { 'X' } else { 'O' })
            .collect();

        match self {
            KeyEvent::Down { .. } => format!("[{}]", keys_str),
            KeyEvent::Up { .. } => format!("[{}]", keys_str),
        }
    }
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
    pub version: Option<u8>,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub profile_tag: Option<String>,
    pub led_enable: Option<bool>,
    pub led_brightness: Option<u8>,
    pub led_mode: Option<u8>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub pointer: Option<Color>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub primary: Option<Color>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub secondary: Option<Color>,
    pub attract_distance: Option<u32>,
    pub feedback_strength: Option<u32>,
    pub bounce_strength: Option<u32>,
    pub haptic_click_strength: Option<u32>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub button_a_idle: Option<Color>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub button_b_idle: Option<Color>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub button_c_idle: Option<Color>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub button_d_idle: Option<Color>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub button_a_press: Option<Color>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub button_b_press: Option<Color>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub button_c_press: Option<Color>,
    #[serde(
        serialize_with = "serialize_color",
        deserialize_with = "deserialize_color"
    )]
    pub button_d_press: Option<Color>,
    pub keys: Option<Vec<KeyDef>>,
    pub knob: Option<Vec<Knob>>,
    pub gui_enable: Option<bool>,
    pub audio: Option<Audio>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KeyDef {
    pub pressed: Vec<KeyPress>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KeyPress {
    #[serde(rename = "type")]
    pub key_type: String,
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
    #[serde(rename = "type")]
    pub knob_type: String,
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

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub led_max_brightness: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_velocity: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_voltage: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_orientation: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi_usb: Option<MidiSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub midi2: Option<MidiSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sysex_id: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MidiSettings {
    #[serde(rename = "in")]
    pub input: bool,
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

fn serialize_color<S>(color: &Option<Color>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match color {
        Some(c) => serializer.serialize_some(&[c.r, c.g, c.b]),
        None => serializer.serialize_none(),
    }
}

fn deserialize_color<'de, D>(deserializer: D) -> Result<Option<Color>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ColorVisitor;

    impl<'de> Visitor<'de> for ColorVisitor {
        type Value = Option<Color>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a u32 color value, an array of 3 u8 values, or null")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(self)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let r = ((value >> 16) & 0xFF) as u8;
            let g = ((value >> 8) & 0xFF) as u8;
            let b = (value & 0xFF) as u8;
            Ok(Some(Color { r, g, b }))
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let r: u8 = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(0, &self))?;
            let g: u8 = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(1, &self))?;
            let b: u8 = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(2, &self))?;
            Ok(Some(Color { r, g, b }))
        }
    }

    deserializer.deserialize_option(ColorVisitor)
}
