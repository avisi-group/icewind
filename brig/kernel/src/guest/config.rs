use {
    crate::{host::fs::Filesystem, util::parse_hex_prefix},
    alloc::{collections::BTreeMap, format, string::String, vec::Vec},
    common::intern::InternedString,
    serde::{Deserialize, Deserializer, de::Error as _},
};

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum ConfigLoadError {
    /// Filesystem error: {0:?}
    Filesystem(crate::host::fs::Error),
    /// Failed to parse JSON config: {0:#?}
    JsonParse(serde_json::Error),
}

impl From<crate::host::fs::Error> for ConfigLoadError {
    fn from(value: crate::host::fs::Error) -> Self {
        Self::Filesystem(value)
    }
}

impl From<serde_json::Error> for ConfigLoadError {
    fn from(value: serde_json::Error) -> Self {
        Self::JsonParse(value)
    }
}

/// Load guest configuration from the config tar
/// image
// pub fn load_from_device(device: &SharedDevice) -> Result<Config,
// ConfigLoadError> {     let mut device = device.lock();
//     let mut fs = TarFilesystem::mount(device.as_block());

//     Ok(serde_json::from_slice(
//         &fs.open("/config.json")?.read_to_vec()?,
//     )?)
// }

pub fn load_from_fs<FS: Filesystem>(fs: &mut FS) -> Result<Config, ConfigLoadError> {
    Ok(serde_json::from_slice(&fs.read_to_vec("/config.json")?)?)
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub memory: BTreeMap<InternedString, AddressSpace>,
    pub load: Vec<Load>,
    pub devices: Vec<Device>,
}

pub type AddressSpace = BTreeMap<InternedString, Memory>;

#[derive(Debug, Deserialize)]
pub struct Memory {
    #[serde(deserialize_with = "hex_address")]
    pub start: u64,
    #[serde(deserialize_with = "hex_address")]
    pub end: u64,
}

#[derive(Debug, Deserialize)]
pub struct Load {
    pub path: InternedString,
    pub kind: LoadKind,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoadKind {
    Elf,
}

#[derive(Debug, Deserialize)]
pub struct Device {
    pub name: InternedString,
    pub kind: InternedString,
    pub attach: Option<DeviceAttachment>,
    #[serde(flatten)]
    pub extra: BTreeMap<InternedString, InternedString>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceAttachment {
    Memory {
        address_space: InternedString,
        #[serde(deserialize_with = "hex_address")]
        base: u64,
    },

    SysReg(BTreeMap<InternedString, [u64; 5]>),
}

/// Function to be passed in `deserialize_with` serde attribute for parsing JSON
/// strings containing hex memory addresses into u64s.
fn hex_address<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let s = String::deserialize(deserializer)?;

    Ok(parse_hex_prefix(&s).map_err(|e| {
        D::Error::custom(format!("Failed to parse u64 from hex string {s:?}: {e:?}"))
    })?)
}

/// Function to be passed in `deserialize_with` serde attribute for parsing JSON
/// strings containing hex memory addresses into u64s.
fn hex_address_opt<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|s| {
            parse_hex_prefix(&s).map_err(|e| {
                D::Error::custom(format!("Failed to parse u64 from hex string {s:?}: {e:?}"))
            })
        })
        .map_or(Ok(None), |v| v.map(Some))
}
