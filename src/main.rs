use anyhow::{anyhow, Context};
use log::{debug, info, warn, error};
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, broadcast};
use radio::{RadioManager, MAX_PACKET_LEN};

mod db;
mod radio;
mod config;
mod ui;

struct AppState {
    conf : config::Config,
    db : db::Database,
    transmit_queue : mpsc::Sender<Vec<u8>>,
    ws_broadcast : broadcast::Sender<ui::UIPacket>,
    start_time : chrono::DateTime<chrono::Utc>,
}

type SharedState = Arc<Mutex<AppState>>;

/* 8191 max packet size would give nearly 32 packets of size 255.
 * Let's leave some space for other whiskers too. */
const TUN_MTU : usize = 24*255;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .env()
        .init().unwrap();

    let conf = config::Config::load().expect("Could not load config");

    let (mut tun_sink, tun_source) = if conf.tunnel.enabled {
        let tunnelconf = conf.tunnel.clone();
        let mut tunconfig = tun::Configuration::default();

        tunconfig
            .address(tunnelconf.local_ip)
            .netmask(tunnelconf.netmask)
            .mtu(TUN_MTU.try_into().unwrap())
            .up();

        #[cfg(target_os = "linux")]
        tunconfig.platform(|tunconfig| {
            tunconfig.packet_information(true);
        });

        let dev = tun::create_as_async(&tunconfig).unwrap();
        use futures::stream::StreamExt;
        let (tun_sink, tun_source) = dev.into_framed().split();
        (Some(tun_sink), Some(tun_source))
    }
    else {
        (None, None)
    };

    let (radio_rx_queue, mut packet_receive) = mpsc::channel(16);
    let (packet_send, mut radio_tx_queue) = mpsc::channel::<Vec<u8>>(16);

    let shared_state = Arc::new(Mutex::new(AppState {
        conf : conf.clone(),
        db : db::Database::new().await,
        transmit_queue : packet_send.clone(),
        ws_broadcast : broadcast::Sender::new(2),
        start_time : chrono::Utc::now(),
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
                // Cut the length prefix, which isn't returned by the real radio
                let packet = buf[2..len].to_vec();
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
            match ham_cats::packet::Packet::fully_decode(&packet_data, &mut buf) {
                Ok(packet) => {
                    let (mut db, ws_broadcast) = {
                        let g = shared_state_receive.lock().unwrap();
                        (g.db.clone(), g.ws_broadcast.clone())
                    };

                    if let Some(ident) = packet.identification() {
                        debug!(" From {}-{}", ident.callsign, ident.ssid);

                        let mut commentbuf = [0u8; 255];
                        match packet.comment(&mut commentbuf) {
                            Ok(comment) => {
                                let m = ui::UIPacket {
                                    received_at: chrono::Utc::now(),
                                    from_callsign: ident.callsign.to_string(),
                                    from_ssid: ident.ssid,
                                    comment: Some(comment.to_owned())
                                };
                                match ws_broadcast.send(m) {
                                    Ok(num) => debug!("Send WS message to {num}"),
                                    Err(_) => debug!("No WS receivers currently"),
                                }
                            }
                            Err(e) => warn!("Decode packet comment error: {e}"),
                        }
                    }

                    if let Err(e) = db.store_packet(&packet_data).await {
                        warn!("Failed to write to sqlite: {}", e);
                    }

                    if let Some(sink) = &mut tun_sink {
                        let mut incoming = Vec::new();
                        for arb in packet.arbitrary_iter() {
                            incoming.extend_from_slice(arb.0.as_slice());
                        }

                        if !incoming.is_empty() {
                            use futures::SinkExt;
                            if let Err(e) = sink.send(tun::TunPacket::new(incoming)).await {
                                warn!("Failed to send to TUN: {}", e);
                            }
                        }
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

    let shared_state_tunnel = shared_state.clone();
    if let Some(mut source) = tun_source {
        tokio::task::spawn(async move {
            use futures::stream::StreamExt;
            while let Some(packet_from_tun) = source.next().await {
                match packet_from_tun {
                    Ok(ip_packet) if ip_packet.get_bytes().len() <= TUN_MTU => {
                        println!("RX: {} bytes", ip_packet.get_bytes().len());

                        let config = shared_state_tunnel.lock().unwrap().conf.clone();

                        fn build_tun_packet(config: config::Config, ip_packet: &[u8]) -> anyhow::Result<Vec<u8>> {
                            let mut buf = [0; MAX_PACKET_LEN];
                            let mut pkt = ham_cats::packet::Packet::new(&mut buf);
                            pkt.add_identification(
                                ham_cats::whisker::Identification::new(&config.callsign, config.ssid, config.icon)
                                .context("Invalid identification")?
                                ).map_err(|e| anyhow!("Could not add identification to packet: {e}"))?;

                            for part in ip_packet.chunks(255) {
                                pkt.add_arbitrary(ham_cats::whisker::Arbitrary::new(part).unwrap())
                                    .map_err(|e| anyhow!("Could not add data to packet: {e}"))?;
                            }

                            let mut buf2 = [0; MAX_PACKET_LEN];
                            let mut data = ham_cats::buffer::Buffer::new_empty(&mut buf2);
                            pkt.fully_encode(&mut data)
                                .map_err(|e| anyhow!("Could not encode packet: {e}"))?;
                            Ok(data.to_vec())
                        }

                        match build_tun_packet(config, ip_packet.get_bytes()) {
                            Ok(data) => if let Err(e) = packet_send.send(data).await {
                                warn!("Failed to send TUN packet: {e}");
                            },
                            Err(e) => warn!("Failed to prepare TUN packet: {e}"),
                        }
                    },
                    Ok(ip_packet) => {
                        println!("RX: too large packet: {} bytes", ip_packet.get_bytes().len());
                    },
                    Err(err) => panic!("Error: {:?}", err),
                }
            }
        });
    }

    let port = 3000;
    info!("Setting up listener on port {port}");
    ui::serve(port, shared_state).await;
    Ok(())
}

