use log::{debug, info, warn, error};
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use radio::{RadioManager, MAX_PACKET_LEN};

mod db;
mod radio;
mod config;
mod ui;

struct AppState {
    conf : config::Config,
    db : db::Database,
    transmit_queue : mpsc::Sender<Vec<u8>>,
}

type SharedState = Arc<Mutex<AppState>>;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    simple_logger::SimpleLogger::new().env().init().unwrap();

    let conf = config::Config::load().expect("Could not load config");

    let (radio_rx_queue, mut packet_receive) = mpsc::channel(16);
    let (packet_send, mut radio_tx_queue) = mpsc::channel::<Vec<u8>>(16);

    let shared_state = Arc::new(Mutex::new(AppState {
        conf : conf.clone(),
        db : db::Database::new().await,
        transmit_queue: packet_send.clone(),
    }));

    if conf.freq == 0 {
        warn!("Frequency {0} is zero, disabling radio. Fake receiver udp 127.0.0.1:9073, sending to 9074", conf.freq);
        let sock_r = Arc::new(UdpSocket::bind("127.0.0.1:9073").await?);
        let sock_s = sock_r.clone();

        // These two tasks behave like the radio, but use UDP instead of the RF channel.
        tokio::spawn(async move {
            let mut buf = [0; 1024];
            while let Ok((len, addr)) = sock_r.recv_from(&mut buf).await {
                println!("{:?} bytes received from {:?}", len, addr);
                let packet = buf[..len].to_vec();
                let rssi = 0f64;
                radio_rx_queue.send((packet, rssi)).await.expect("Inject frame");
            }
        });

        tokio::spawn(async move {
            while let Some(p) = radio_tx_queue.recv().await {
                sock_s.send_to(&p, "127.0.0.1:9074").await.unwrap();
            }
        });
    }
    else if !(430000..=436380).contains(&conf.freq) {
        error!("Frequency {} kHz out of range (430MHz - 436.375MHz), skipping radio setup", conf.freq);
    }
    else {
        info!("Setting up radio");
        let mut radio = RadioManager::new(radio_rx_queue, radio_tx_queue).expect("Could not initialize radio");

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
    }

    let shared_state_receive = shared_state.clone();
    tokio::task::spawn(async move {
        while let Some((packet_data, rssi)) = packet_receive.recv().await {
            debug!("RX RSSI {} len {}", rssi, packet_data.len());
            let mut buf = [0; MAX_PACKET_LEN];
            match ham_cats::packet::Packet::fully_decode(&packet_data[2..], &mut buf) {
                Ok(packet) => {
                    if let Some(ident) = packet.identification() {
                        debug!(" From {}-{}", ident.callsign, ident.ssid);
                    }

                    let mut db = shared_state_receive.lock().unwrap().db.clone();
                    if let Err(e) = db.store_packet(&packet_data).await {
                        warn!("Failed to write to sqlite: {}", e);
                    }
                }
                Err(e) => {
                    warn!("Failed to decode packet: {}", e);
                    eprintln!("{:02X?}", packet_data);
                }
            }
        }

        warn!("Packet receive task stopping");
    });

    let port = 3000;
    info!("Setting up listener on port {port}");
    ui::serve(port, shared_state).await;
    Ok(())
}

