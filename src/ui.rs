use std::str::FromStr;
use serde::Deserialize;
use askama::Template;
use axum::{
    extract::State,
    routing::get,
    Router,
    response::Html,
    Form,
    http::StatusCode,
};
use tower_http::services::ServeDir;

use crate::config;
use crate::SharedState;

pub async fn serve(port: u16, shared_state: SharedState) {
    let app = Router::new()
        .route("/", get(dashboard))
        .route("/incoming", get(incoming))
        .route("/send", get(send))
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
}

async fn dashboard(State(state): State<SharedState>) -> DashboardTemplate<'static> {
    DashboardTemplate {
        title: "Dashboard",
        conf: state.lock().unwrap().conf.clone(),
        page: ActivePage::Dashboard,
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
    if value == "" {
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
