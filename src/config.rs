use std::fs;
use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FelinetConfig {
    pub enabled: bool,
    pub address: String,
}

impl Default for FelinetConfig {
    fn default() -> Self {
        FelinetConfig {
            enabled: false,
            address: "https://felinet.cats.radio".to_owned()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelConfig {
    pub enabled: bool,
    pub local_ip: String,
    pub netmask: String,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        TunnelConfig {
            enabled: false,
            local_ip: "10.73.14.1".to_owned(),
            netmask: "255.255.255.0".to_owned(),
        }
    }
}

type DurationSeconds = std::num::NonZeroU32;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BeaconConfig {
    pub period_seconds: Option<DurationSeconds>,
    #[serde(default)]
    pub max_hops: u8,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,
    pub comment: Option<String>,
    pub antenna_height: Option<u8>,
    pub antenna_gain: Option<f32>,
    pub tx_power: Option<f32>,
}

impl Default for BeaconConfig {
    fn default() -> Self {
        BeaconConfig {
            period_seconds: None,
            max_hops: 3,
            latitude: None,
            longitude: None,
            altitude: None,
            comment: None,
            antenna_height: None,
            antenna_gain: None,
            tx_power: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub callsign: String,
    pub ssid: u8,
    #[serde(default)]
    pub icon: u16,
    pub felinet: FelinetConfig,
    pub beacon: BeaconConfig,
    pub tunnel: TunnelConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            callsign: "CHANGEME".to_owned(),
            ssid: 0,
            icon: 0,
            felinet: Default::default(),
            beacon: Default::default(),
            tunnel: Default::default(),
        }
    }
}

const CONFIGFILE : &str = "node-config.toml";

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        if std::path::Path::new(CONFIGFILE).exists() {
            let file_contents = fs::read_to_string(CONFIGFILE)?;
            toml::from_str(&file_contents).context("parsing config file")
        }
        else {
            Ok(Default::default())
        }
    }

    pub fn store(&self) -> anyhow::Result<()> {
        fs::write(CONFIGFILE, toml::to_string_pretty(&self)?)
            .context("writing config file")
    }
}
