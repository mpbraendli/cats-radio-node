use anyhow::{anyhow, Context};
use ham_cats::{
    buffer::Buffer,
    whisker::{Arbitrary, Identification, Gps},
};

const MAX_PACKET_LEN : usize = 8191;

fn build_example_packet(comment: &str) -> anyhow::Result<Vec<u8>> {
    let callsign = "EX4MPLE";
    let ssid = 0;
    let icon = 0;

    let mut buf = [0; MAX_PACKET_LEN];
    let mut pkt = ham_cats::packet::Packet::new(&mut buf);
    pkt.add_identification(
        Identification::new(callsign, ssid, icon)
            .context("Invalid identification")?,
    )
    .map_err(|e| anyhow!("Could not add identification to packet: {e}"))?;

    pkt.add_comment(comment)
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

fn main() -> std::io::Result<()> {
    std::thread::spawn(receive_loop);

    eprintln!("Receiving messages. Write a comment and press ENTER to send. Ctrl-C to stop");
    let mut stdin_lines = std::io::stdin().lines();
    let sock = std::net::UdpSocket::bind("127.0.0.1:9075").unwrap();

    while let Some(Ok(line)) = stdin_lines.next() {
        eprintln!("Sending with comment = {}", line);

        let packet = build_example_packet(&line).unwrap();
        sock.send_to(&packet, "127.0.0.1:9073").unwrap();
    }

    Ok(())
}

fn receive_loop() {
    let sock = std::net::UdpSocket::bind("127.0.0.1:9074").unwrap();
    let mut data = [0; MAX_PACKET_LEN];
    while let Ok((len, _addr)) = sock.recv_from(&mut data) {
        eprintln!("Packet of length {}", len);

        let mut buf = [0; MAX_PACKET_LEN];
        match ham_cats::packet::Packet::fully_decode(&data[2..len], &mut buf) {
            Ok(packet) => {
                if let Some(ident) = packet.identification() {
                    eprintln!(" Ident {}-{}", ident.callsign, ident.ssid);
                }

                for dest in packet.destination_iter() {
                    eprintln!(" TO {}-{}", dest.callsign(), dest.ssid());
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
                eprintln!(" Cannot decode {:?}", e);
            }
        }
    }
}

