use anyhow::Context;
use log::{debug, info, warn, error};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use sqlx::{Connection, SqliteConnection};
use radio::{RadioManager, MAX_PACKET_LEN};

mod radio;
mod config;
mod ui;

struct AppState {
    conf : config::Config,
    db : Mutex<SqliteConnection>
}

type SharedState = Arc<Mutex<AppState>>;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    simple_logger::SimpleLogger::new().env().init().unwrap();

    let mut conn = SqliteConnection::connect("sqlite:cats-radio-node.db").await.unwrap();
    sqlx::migrate!()
        .run(&mut conn)
        .await
        .expect("could not run SQLx migrations");

    let conf = config::Config::load().expect("Could not load config");

    if conf.freq == 0 {
        warn!("Frequency {0} is zero, disabling radio", conf.freq);
    }
    else if !(430000..=436380).contains(&conf.freq) {
        error!("Frequency {} kHz out of range (430MHz - 436.375MHz), skipping radio setup", conf.freq);
    }
    else {
        info!("Setting up radio");
        let (packet_tx, mut packet_receive) = mpsc::channel(16);
        let (packet_send, packet_rx) = mpsc::channel(16);
        let mut radio = RadioManager::new(packet_tx, packet_rx).expect("Could not initialize radio");

        let channel = ((conf.freq - 430000) / 25) as u8;
        radio.set_channel(channel);
        let actual_freq = 430000 + 25 * channel as u32;
        info!("Setting up radio on {actual_freq} kHz...");

        tokio::task::spawn(async move {
            loop {
                if let Err(e) = radio.process_forever().await {
                    error!("Radio error: {e}")
                }
            }
        });

        tokio::task::spawn(async move {
            loop {
                match packet_receive
                    .recv()
                    .await
                    .context("Packet receive channel died") {
                        Ok((packet, rssi)) => {
                            debug!("RX RSSI {} len {}", rssi, packet.len());
                            let mut buf = [0; MAX_PACKET_LEN];
                            match ham_cats::packet::Packet::fully_decode(&packet, &mut buf) {
                                Ok(packet) => {
                                    if let Some(ident) = packet.identification() {
                                        debug!(" From {}-{}", ident.callsign, ident.ssid);
                                    }
                                    // TODO save to db
                                }
                                Err(e) => {
                                    warn!("Failed to decode packet: {}", e);
                                }
                            }
                        },
                        Err(e) => warn!("Failed to decode packet: {}", e),
                    }
            }
        });
    }

    let shared_state = Arc::new(Mutex::new(AppState {
        conf,
        db: Mutex::new(conn)
    }));

    let port = 3000;
    info!("Setting up listener on port {port}");
    ui::serve(port, shared_state).await;
    Ok(())
}

