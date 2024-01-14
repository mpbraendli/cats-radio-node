use anyhow::{anyhow, Context};
use std::str::FromStr;
use axum::Json;
use log::{info, warn};
use serde::Deserialize;
use askama::Template;
use axum::{
    extract::State,
    routing::{get, post},
    Router,
    response::Html,
    Form,
    http::StatusCode,
};
use tower_http::services::ServeDir;

use ham_cats::{
    buffer::Buffer,
    whisker::Identification,
};

use crate::{config, radio::MAX_PACKET_LEN};
use crate::SharedState;

pub async fn serve(port: u16, shared_state: SharedState) {
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/incoming", get(incoming))
        .route("/send", get(send))
        .route("/api/send_packet", post(post_packet))
        .route("/settings", get(show_settings).post(post_settings))
        .nest_service("/static", ServeDir::new("static"))
        /* For an example for timeouts and tracing, have a look at the git history */
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    axum::serve(listener, app).await.unwrap()
}

#[derive(PartialEq)]
enum ActivePage {
    Dashboard,
    Incoming,
    Send,
    Settings,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    conf: config::Config,
    packets: Vec<UIPacket>,
}

struct UIPacket {
    pub received_at : i64,

    pub from_callsign : String,
    pub from_ssid : u8,

    pub comment : Option<String>,
}

async fn dashboard(State(state): State<SharedState>) -> DashboardTemplate<'static> {
    let mut db = state.lock().unwrap().db.clone();

    let packets = match db.get_most_recent_packets(10).await {
        Ok(v) => v,
        Err(e) => {
            warn!("Dashboard will have empty packet list: {}", e);
            Vec::new()
        },
    }.iter()
    .filter_map(|db_packet| {
        let mut buf = [0; MAX_PACKET_LEN];
        match ham_cats::packet::Packet::fully_decode(&db_packet.content, &mut buf) {
            Ok(p) => {
                if let Some(ident) = p.identification() {

                    let mut commentbuf = [0; 1024];
                    let comment = match p.comment(&mut commentbuf) {
                        Ok(c) => Some(c.to_owned()),
                        Err(_) => None,
                    };

                    Some(UIPacket {
                        received_at : db_packet.received_at,
                        from_callsign : ident.callsign.to_string(),
                        from_ssid : ident.ssid,
                        comment
                    })
                }
                else {
                    None
                }
            },
            Err(e) => {
                warn!("Failed to decode packet {}: {}", db_packet.id, e);
                None
            },
        }
    })
    .collect();

    DashboardTemplate {
        title: "Dashboard",
        conf: state.lock().unwrap().conf.clone(),
        page: ActivePage::Dashboard,
        packets
    }
}

#[derive(Template)]
#[template(path = "incoming.html")]
struct IncomingTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    conf: config::Config,
}

async fn incoming(State(state): State<SharedState>) -> IncomingTemplate<'static> {
    IncomingTemplate {
        title: "Incoming",
        conf: state.lock().unwrap().conf.clone(),
        page: ActivePage::Incoming,
    }
}

#[derive(Template)]
#[template(path = "send.html")]
struct SendTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    conf: config::Config,
}

async fn send(State(state): State<SharedState>) -> SendTemplate<'static> {
    SendTemplate {
        title: "Send",
        conf: state.lock().unwrap().conf.clone(),
        page: ActivePage::Send,
    }
}

#[derive(Deserialize, Debug)]
struct ApiSendPacket {
    comment : Option<String>,
}

fn build_packet(config: config::Config, comment: Option<String>) -> anyhow::Result<Vec<u8>> {
    let mut buf = [0; crate::radio::MAX_PACKET_LEN];
    let mut pkt = ham_cats::packet::Packet::new(&mut buf);
    pkt.add_identification(
        Identification::new(&config.callsign, config.ssid, config.icon)
            .context("Invalid identification")?,
    )
    .map_err(|e| anyhow!("Could not add identification to packet: {e}"))?;

    if let Some(c) = comment {
        pkt.add_comment(&c)
            .map_err(|e| anyhow!("Could not add comment to packet: {e}"))?;
    }

    let mut buf2 = [0; crate::radio::MAX_PACKET_LEN];
    let mut data = Buffer::new_empty(&mut buf2);
    pkt.fully_encode(&mut data)
        .map_err(|e| anyhow!("Could not encode packet: {e}"))?;

    Ok(data.to_vec())
}

async fn post_packet(State(state): State<SharedState>, Json(payload): Json<ApiSendPacket>) -> StatusCode {
    let (config, transmit_queue) = {
        let s = state.lock().unwrap();
        (s.conf.clone(), s.transmit_queue.clone())
    };

    info!("send_packet {:?}", payload);

    match build_packet(config, payload.comment) {
        Ok(p) => {
            info!("Built packet of {} bytes", p.len());
            match transmit_queue.send(p).await {
                Ok(()) => StatusCode::OK,
                Err(_) => StatusCode::BAD_REQUEST,
            }
        },
        Err(_) =>StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    conf: config::Config,
}

async fn show_settings(State(state): State<SharedState>) -> SettingsTemplate<'static> {
    SettingsTemplate {
        title: "Settings",
        page: ActivePage::Settings,
        conf: state.lock().unwrap().conf.clone(),
    }
}

#[derive(Deserialize, Debug)]
struct FormConfig {
    freq: String,
    callsign: String,
    ssid: String,
    icon: String,

    // felinet
    // felinet_enabled is either "on" or absent.
    // According to https://developer.mozilla.org/en-US/docs/Web/HTML/Element/Input/checkbox
    // "If the value attribute was omitted, the default value for the checkbox is `on` [...]"
    felinet_enabled: Option<String>,
    address: String,

    // beacon
    period_seconds: config::DurationSeconds,
    max_hops: u8,
    latitude: String,
    longitude: String,
    altitude: String,
    comment: String,
    antenna_height: String,
    antenna_gain: String,
    tx_power: String,

    // tunnel
    tunnel_enabled: Option<String>,
    local_ip: String,
    netmask: String,
}

fn empty_string_to_none<T: FromStr + Sync>(value: &str) -> Result<Option<T>, T::Err> {
    if value.is_empty() {
        Ok(None)
    }
    else {
        Ok(Some(value.parse()?))
    }
}

impl TryFrom<FormConfig> for config::Config {
    type Error = anyhow::Error;

    fn try_from(value: FormConfig) -> Result<Self, Self::Error> {
        Ok(config::Config {
            freq: value.freq.parse()?,
            callsign: value.callsign,
            ssid: value.ssid.parse()?,
            icon: value.icon.parse()?,
            felinet: config::FelinetConfig {
                enabled: value.felinet_enabled.is_some(),
                address: value.address,
            },
            beacon: config::BeaconConfig {
                period_seconds: value.period_seconds,
                max_hops: value.max_hops,
                latitude: empty_string_to_none(&value.latitude)?,
                longitude: empty_string_to_none(&value.longitude)?,
                altitude: empty_string_to_none(&value.altitude)?,
                comment: empty_string_to_none(&value.comment)?,
                antenna_height: empty_string_to_none(&value.antenna_height)?,
                antenna_gain: empty_string_to_none(&value.antenna_gain)?,
                tx_power: empty_string_to_none(&value.tx_power)?,
            },
            tunnel: config::TunnelConfig {
                enabled: value.tunnel_enabled.is_some(),
                local_ip: value.local_ip,
                netmask: value.netmask,
            },
        })
    }
}

async fn post_settings(State(state): State<SharedState>, Form(input): Form<FormConfig>) -> (StatusCode, Html<String>) {
    match config::Config::try_from(input) {
        Ok(c) => {
            match c.store() {
                Ok(()) => {
                    state.lock().unwrap().conf.clone_from(&c);

                    (StatusCode::OK, Html(
                            r#"<!doctype html>
                            <html><head></head><body>
                            <p>Configuration updated</p>
                            <p>To <a href="/">dashboard</a></p>
                            </body></html>"#.to_owned()))
                }
                Err(e) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, Html(
                            format!(r#"<!doctype html>
                            <html><head></head>
                            <body><p>Internal Server Error: Could not write config</p>
                            <p>{}</p>
                            </body>
                            </html>"#, e)))
                },
            }

        },
        Err(e) => {
            (StatusCode::BAD_REQUEST, Html(
                    format!(r#"<!doctype html>
                            <html><head></head>
                            <body><p>Error interpreting POST data</p>
                            <p>{}</p>
                            </body>
                            </html>"#, e)))
        },
    }
}
