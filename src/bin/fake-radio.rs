use anyhow::{anyhow, Context};
use tokio::net::UdpSocket;
use ham_cats::{
    buffer::Buffer,
    whisker::{Arbitrary, Identification, Gps},
};

const MAX_PACKET_LEN : usize = 8191;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    eprintln!("Sending example packet");

    let packet = build_example_packet().await.unwrap();
    let sock = UdpSocket::bind("127.0.0.1:9074").await.unwrap();
    sock.send_to(&packet, "127.0.0.1:9073").await.unwrap();

    eprintln!("Receiving packets. Ctrl-C to stop");

    let mut data = [0; MAX_PACKET_LEN];
    while let Ok((len, _addr)) = sock.recv_from(&mut data).await {
        let mut buf = [0; MAX_PACKET_LEN];
        match ham_cats::packet::Packet::fully_decode(&data[2..len], &mut buf) {
            Ok(packet) => {
                if let Some(ident) = packet.identification() {
                    eprintln!(" Ident {}-{}", ident.callsign, ident.ssid);
                }

                if let Some(gps) = packet.gps() {
                    eprintln!(" GPS {} {}", gps.latitude(), gps.longitude());
                }

                let mut comment = [0; 1024];
                if let Ok(c) = packet.comment(&mut comment) {
                    eprintln!(" Comment {}", c);
                }

                eprintln!(" With {} Arbitrary whiskers", packet.arbitrary_iter().count());
            },
            Err(e) => {
                eprintln!(" Cannot decode packet of length {} {:?}", len, e);
            }
        }
    }

    Ok(())
}

async fn build_example_packet() -> anyhow::Result<Vec<u8>> {
    let callsign = "EX4MPLE";
    let ssid = 0;
    let icon = 0;

    let mut buf = [0; MAX_PACKET_LEN];
    let mut pkt = ham_cats::packet::Packet::new(&mut buf);
    pkt.add_identification(
        Identification::new(&callsign, ssid, icon)
            .context("Invalid identification")?,
    )
    .map_err(|e| anyhow!("Could not add identification to packet: {e}"))?;

    pkt.add_comment("Debugging packet")
        .map_err(|e| anyhow!("Could not add comment to packet: {e}"))?;

    let latitude = 46.5;
    let longitude = -6.2;
    let altitude = 200u8.into();
    let max_error = 1;
    let heading = 120.0;
    let speed = 1u8.into();

    pkt.add_gps(Gps::new(
        latitude,
        longitude,
        altitude,
        max_error,
        heading,
        speed)
    )
    .map_err(|e| anyhow!("Could not add GPS to packet: {e}"))?;

    pkt.add_arbitrary(Arbitrary::new(&[0xA5; 8]).unwrap())
        .map_err(|e| anyhow!("Could not add arbitrary to packet: {e}"))?;

    let mut buf2 = [0; MAX_PACKET_LEN];
    let mut data = Buffer::new_empty(&mut buf2);
    pkt.fully_encode(&mut data)
        .map_err(|e| anyhow!("Could not encode packet: {e}"))?;

    Ok(data.to_vec())
}
