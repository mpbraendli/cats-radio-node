use std::time::{UNIX_EPOCH, SystemTime, Duration};
use std::ops::ControlFlow;
use std::net::SocketAddr;
use std::str::FromStr;
use anyhow::{anyhow, Context};
use askama::Template;
use axum::{
    Form,
    Json,
    Router,
    extract::State,
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, ConnectInfo},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::serde::ts_seconds;
use futures::{StreamExt, SinkExt};
use log::{debug, info, warn, error};
use serde::Deserialize;
use tower_http::services::ServeDir;

use ham_cats::{
    buffer::Buffer,
    whisker::{Identification, Destination},
};

use crate::{config, radio::MAX_PACKET_LEN};
use crate::SharedState;

pub async fn serve(port: u16, shared_state: SharedState) {
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/chat", get(chat))
        .route("/chat/ws", get(ws_handler))
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
    Chat,
    Send,
    Settings,
    None,
}

impl ActivePage {
    // Used by templates/head.html to include the correct js files in <head>
    fn styles(&self) -> Vec<&'static str> {
        match self {
            ActivePage::Dashboard => vec![],
            ActivePage::Chat => vec!["chat.js"],
            ActivePage::Send => vec!["send.js"],
            ActivePage::Settings => vec![],
            ActivePage::None => vec![],
        }
    }
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    conf: config::Config,
    node_startup_time: String,
    num_received_frames: u64,
    packets: Vec<UIPacket>,
}

#[derive(Clone, serde::Serialize)]
pub struct UIPacket {
    #[serde(with = "ts_seconds")]
    pub received_at: chrono::DateTime<chrono::Utc>,

    pub from_callsign : String,
    pub from_ssid : u8,

    pub comment : Option<String>,
}

impl UIPacket {
    fn received_at_iso(&self) -> String {
        self.received_at.to_string()
    }

    fn from_db_packet(db_packet: &crate::db::Packet) -> Option<Self> {
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
    }
}

async fn dashboard(State(state): State<SharedState>) -> DashboardTemplate<'static> {
    let (conf, mut db, node_startup_time) = {
        let st = state.lock().unwrap();
        (st.conf.clone(), st.db.clone(), st.start_time.clone())
    };

    let packets = match db.get_most_recent_packets(10).await {
        Ok(v) => v,
        Err(e) => {
            warn!("Dashboard will have empty packet list: {}", e);
            Vec::new()
        },
    }.iter()
    .filter_map(|p| UIPacket::from_db_packet(p))
    .collect();

    let node_startup_time = format!("{} UTC",
        node_startup_time.format("%Y-%m-%d %H:%M:%S"));

    DashboardTemplate {
        title: "Dashboard",
        conf,
        page: ActivePage::Dashboard,
        num_received_frames : db.get_num_received_frames(),
        node_startup_time,
        packets,
    }
}

#[derive(Template)]
#[template(path = "chat.html")]
struct ChatTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    conf: config::Config,
    packets: Vec<UIPacket>,
}

async fn chat(State(state): State<SharedState>) -> ChatTemplate<'static> {

    let (conf, mut db) = {
        let st = state.lock().unwrap();
        (st.conf.clone(), st.db.clone())
    };

    let time_start = SystemTime::now() - Duration::from_secs(6*3600);
    let timestamp = time_start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    let timestamp_i64 : i64 = timestamp.as_secs().try_into().unwrap();
    let packets = match db.get_packets_since(timestamp_i64).await {
        Ok(packets) => {
            packets.iter()
                .filter_map(|p| UIPacket::from_db_packet(p))
                .collect()
        },
        Err(e) => {
            error!("Failed to get packets since TS: {e}");
            vec![]
        }
    };

    ChatTemplate {
        title: "Chat",
        conf,
        page: ActivePage::Chat,
        packets
    }
}

async fn ws_handler(
    State(state): State<SharedState>,
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>) -> impl IntoResponse {
    info!("User at {addr} connected.");
    let rx = state.lock().unwrap().ws_broadcast.subscribe();
    ws.on_upgrade(move |socket| handle_socket(socket, rx, addr))
}

async fn handle_socket(
    mut socket: WebSocket,
    mut rx: tokio::sync::broadcast::Receiver<UIPacket>,
    who: SocketAddr) {
    if socket.send(Message::Ping(vec![1, 2, 3])).await.is_ok() {
        info!("Pinged {who}...");
    } else {
        info!("Could not ping {who}!");
        return;
    }
    let (mut sender, mut receiver) = socket.split();

    let mut send_task = tokio::spawn(async move {
        while let Ok(m) = rx.recv().await {
            if let Ok(m_json) = serde_json::to_string(&m) {
                if sender
                    .send(Message::Text(m_json))
                        .await
                        .is_err()
                {
                    return;
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if process_message(msg, who).is_break() {
                break;
            }
        }
    });

    // If any one of the tasks exit, abort the other.
    tokio::select! {
        _rv_a = (&mut send_task) => {
            recv_task.abort();
        },
        _rv_b = (&mut recv_task) => {
            send_task.abort();
        }
    }

    info!("Websocket context {who} destroyed");
}

fn process_message(msg: Message, who: SocketAddr) -> ControlFlow<(), ()> {
    match msg {
        Message::Text(t) => {
            debug!(">>> {who} sent str: {t:?}");
        }
        Message::Binary(d) => {
            debug!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
        }
        Message::Close(c) => {
            if let Some(cf) = c {
                debug!(
                    ">>> {} sent close with code {} and reason `{}`",
                    who, cf.code, cf.reason
                );
            } else {
                debug!(">>> {who} somehow sent close message without CloseFrame");
            }
            return ControlFlow::Break(());
        }

        Message::Pong(v) => {
            debug!(">>> {who} sent pong with {v:?}");
        }
        // You should never need to manually handle Message::Ping, as axum's websocket library
        // will do so for you automagically by replying with Pong and copying the v according to
        // spec. But if you need the contents of the pings you can see them here.
        Message::Ping(v) => {
            debug!(">>> {who} sent ping with {v:?}");
        }
    }
    ControlFlow::Continue(())
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
struct ApiSendPacketDestination {
    callsign : String,
    ssid : u8,
}

#[derive(Deserialize, Debug)]
struct ApiSendPacket {
    destinations : Vec<ApiSendPacketDestination>,
    comment : Option<String>,
}

fn build_packet(config: config::Config, payload: ApiSendPacket) -> anyhow::Result<Vec<u8>> {
    let mut buf = [0; crate::radio::MAX_PACKET_LEN];
    let mut pkt = ham_cats::packet::Packet::new(&mut buf);
    pkt.add_identification(
        Identification::new(&config.callsign, config.ssid, config.icon)
            .context("Invalid identification")?,
    )
    .map_err(|e| anyhow!("Could not add identification to packet: {e}"))?;

    if let Some(c) = payload.comment {
        pkt.add_comment(&c)
            .map_err(|e| anyhow!("Could not add comment to packet: {e}"))?;
    }

    for dest in payload.destinations {
        let dest = Destination::new(false, 0, &dest.callsign, dest.ssid)
            .ok_or(anyhow!("Cound not create destination"))?;

        pkt.add_destination(dest)
            .map_err(|e| anyhow!("Could not add destination to packet: {e}"))?;
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

    match build_packet(config, payload) {
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

#[derive(Template)]
#[template(path = "settings_applied.html")]
struct SettingsAppliedTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    conf: config::Config,
    ok: bool,
    error_message: &'a str,
    error_reason: String,
}

async fn post_settings(
    State(state): State<SharedState>,
    Form(input): Form<FormConfig>) -> (StatusCode, SettingsAppliedTemplate<'static>) {

    match config::Config::try_from(input) {
        Ok(c) => {
            match c.store() {
                Ok(()) => {
                    state.lock().unwrap().conf.clone_from(&c);

                    (StatusCode::OK, SettingsAppliedTemplate {
                        title: "Settings",
                        conf: c,
                        page: ActivePage::None,
                        ok: true,
                        error_message: "",
                        error_reason: "".to_owned(),
                    })
                }
                Err(e) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, SettingsAppliedTemplate {
                        title: "Settings",
                        conf : c,
                        page: ActivePage::None,
                        ok: false,
                        error_message: "Failed to store config",
                        error_reason: e.to_string(),
                    })
                },
            }
        },
        Err(e) => {
            (StatusCode::BAD_REQUEST, SettingsAppliedTemplate {
                        title: "Settings",
                        conf: state.lock().unwrap().conf.clone(),
                        page: ActivePage::None,
                        ok: false,
                        error_message: "Error interpreting POST data",
                        error_reason: e.to_string(),
                    })
        },
    }
}
