use std::fs;
use anyhow::Context;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct FelinetConfig {
    pub enabled: bool,
    pub address: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TunnelConfig {
    pub enabled: bool,
    pub local_ip: String,
    pub netmask: String,
}

type DurationSeconds = std::num::NonZeroU32;

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub callsign: String,
    pub ssid: u8,
    #[serde(default)]
    pub icon: u16,
    pub felinet: FelinetConfig,
    pub beacon: BeaconConfig,
    pub tunnel: Option<TunnelConfig>,
}

const CONFIGFILE : &str = "node-config.toml";

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let file_contents = fs::read_to_string(CONFIGFILE)?;
        toml::from_str(&file_contents).context("parsing config file")
    }

    pub fn store(&self) -> anyhow::Result<()> {
        fs::write(CONFIGFILE, toml::to_string_pretty(&self)?)
            .context("writing config file")
    }
}
