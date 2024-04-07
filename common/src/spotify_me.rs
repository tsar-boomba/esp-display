use rspotify::{Context, Device, RepeatState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    pub short_term_top: Vec<SimpleTrack>,
    pub mid_term_top: Vec<SimpleTrack>,
    pub long_term_top: Vec<SimpleTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleArtist {
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleTrack {
    pub name: String,
    pub artists: Vec<SimpleArtist>,
    pub image_url: Option<String>,
    pub small_url: Option<String>,
    pub url: Option<String>,
    pub duration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playing {
    pub device: Device,
    pub context: Option<Context>,
    pub repeat: RepeatState,
    pub shuffled: bool,
    pub playing: SimpleTrack,
    pub progress_secs: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPlayed {
    pub track: SimpleTrack,
    pub context: Option<Context>,
    pub played_at: String,
}

/// Hold copies of types from `rspotify` which are needed to use my Spotify API
mod rspotify {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};
    use strum::{Display, EnumString, IntoStaticStr};

    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    pub struct Context {
        /// The URI may be of any type, so it's not parsed into a [`crate::Id`]
        pub uri: String,
        pub href: String,
        pub external_urls: HashMap<String, String>,
        #[serde(rename = "type")]
        pub _type: Type,
    }

    /// Device object
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    pub struct Device {
        pub id: Option<String>,
        pub is_active: bool,
        pub is_private_session: bool,
        pub is_restricted: bool,
        pub name: String,
        #[serde(rename = "type")]
        pub _type: DeviceType,
        pub volume_percent: Option<u32>,
    }

    /// Repeat state: `track`, `context` or `off`.
    #[derive(Clone, Debug, Copy, Serialize, Deserialize, PartialEq, Eq, IntoStaticStr)]
    #[serde(rename_all = "snake_case")]
    #[strum(serialize_all = "snake_case")]
    pub enum RepeatState {
        Off,
        Track,
        Context,
    }

    /// Device Type: `computer`, `smartphone`, `speaker`, `TV`
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, IntoStaticStr)]
    #[strum(serialize_all = "snake_case")]
    pub enum DeviceType {
        Computer,
        Tablet,
        Smartphone,
        Speaker,
        /// Though undocumented, it has been reported that the Web API returns both
        /// 'Tv' and 'TV' as the type.
        #[serde(alias = "TV")]
        Tv,
        /// Same as above, the Web API returns both 'AVR' and 'Avr' as the type.
        #[serde(alias = "AVR")]
        Avr,
        /// Same as above, the Web API returns both 'STB' and 'Stb' as the type.
        #[serde(alias = "STB")]
        Stb,
        AudioDongle,
        GameConsole,
        CastVideo,
        CastAudio,
        Automobile,
        Unknown,
    }

    /// Type: `artist`, `album`, `track`, `playlist`, `show` or `episode`
    #[derive(
        Clone,
        Serialize,
        Deserialize,
        Copy,
        PartialEq,
        Eq,
        Debug,
        Display,
        EnumString,
        IntoStaticStr,
    )]
    #[serde(rename_all = "snake_case")]
    #[strum(serialize_all = "snake_case")]
    pub enum Type {
        Artist,
        Album,
        Track,
        Playlist,
        User,
        Show,
        Episode,
        Collection,
        Collectionyourepisodes, // rename to collectionyourepisodes
    }
}
