use anyhow::{anyhow, bail, Context};
use rand::{thread_rng, Rng};
use rf4463::{config::RADIO_CONFIG_CATS, Rf4463};
use rppal::{
    gpio::{Gpio, OutputPin},
    hal::Delay,
    spi::{Bus, Mode, SlaveSelect, Spi},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{
    mpsc::{error::TryRecvError, Receiver, Sender},
    Mutex,
};

pub const MAX_PACKET_LEN: usize = 8191;

pub struct RadioManager {
    radio: Rf4463<Spi, OutputPin, OutputPin, Delay>,

    receive_queue: Sender<(Vec<u8>, f64)>,
    transmit_queue: Receiver<Vec<u8>>,
    rx_buf: [u8; MAX_PACKET_LEN],
    temperature: Arc<Mutex<f32>>,
}

impl RadioManager {
    pub fn new(receive_queue: Sender<(Vec<u8>, f64)>, transmit_queue: Receiver<Vec<u8>>) -> anyhow::Result<Self> {
        let spi = Spi::new(Bus::Spi0, SlaveSelect::Ss0, 1_000_000, Mode::Mode0)?;
        let gpio = Gpio::new()?;
        let sdn = gpio.get(22)?.into_output();
        let cs = gpio.get(24)?.into_output();

        let delay = Delay::new();

        let mut radio = Rf4463::new(spi, sdn, cs, delay, &mut RADIO_CONFIG_CATS.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        radio.set_channel(20);

        let rx_buf = [0; MAX_PACKET_LEN];
        let temperature = Arc::new(Mutex::new(radio.get_temp()?));

        Ok(Self {
            radio,
            receive_queue,
            transmit_queue,
            rx_buf,
            temperature,
        })
    }

    pub fn set_channel(&mut self, channel: u8) {
        self.radio.set_channel(channel);
    }

    pub fn temperature_mutex(&self) -> Arc<Mutex<f32>> {
        self.temperature.clone()
    }

    pub async fn process_forever(&mut self) -> anyhow::Result<()> {
        loop {
            self.tick().await?;

            *self.temperature.lock().await = self.radio.get_temp()?;

            match self.transmit_queue.try_recv() {
                Ok(pkt) => {
                    self.tx(&pkt).await?;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    bail!("TX channel disconnected")
                }
            }

            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn tick(&mut self) -> anyhow::Result<()> {
        if self.radio.is_idle() {
            self.radio
                .start_rx(None, false)
                .map_err(|e| anyhow!("{e}"))?;

            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        self.radio
            .interrupt(Some(&mut self.rx_buf), None)
            .map_err(|e| anyhow!("{e:?}"))?;

        if let Some(data) = self
            .radio
            .finish_rx(&mut self.rx_buf)
            .map_err(|e| anyhow!("{e}"))?
        {
            self.radio
                .start_rx(None, false)
                .map_err(|e| anyhow!("{e}"))?;

            self.receive_queue
                .send((data.data().to_vec(), data.rssi()))
                .await
                .ok()
                .context("RX channel died")?;
        }

        Ok(())
    }

    async fn tx(&mut self, data: &[u8]) -> anyhow::Result<()> {
        // ensures we don't tx over a packet,
        // and adds some random delay so that every node
        // if offset slightly
        self.tx_delay().await?;

        self.radio.start_tx(data).map_err(|e| anyhow!("{e:?}"))?;

        const TIMEOUT: Duration = Duration::from_secs(10);
        let start_time = Instant::now();
        while !self.radio.is_idle() {
            self.radio
                .interrupt(None, Some(data))
                .map_err(|e| anyhow!("{e:?}"))?;

            if start_time + TIMEOUT < Instant::now() {
                bail!("Timeout while transmitting");
            }

            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        Ok(())
    }

    async fn tx_delay(&mut self) -> anyhow::Result<()> {
        loop {
            let delay_ms = thread_rng().gen_range(0..50);

            // since delay_ms < 100 we can safely sleep without calling tick
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;

            let mut rx = false;

            while self.radio.is_busy_rxing()? {
                rx = true;
                self.tick().await?;

                tokio::time::sleep(Duration::from_millis(25)).await;
            }

            if !rx {
                // didn't rx a packet, so we're safe to leave
                break Ok(());
            }
        }
    }
}
