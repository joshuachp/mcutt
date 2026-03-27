use std::time::Duration;
use std::time::Instant;

use mcutt::sync::Connection;
use mcutt::v3::packets::connect::KeepAlive;
use mcutt::v3::packets::connect::builder::ConnectBuilder;
use mcutt::v3::packets::publish::builder::PublishBuilder;
use tracing::debug;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    tracing_subscriber::registry()
        .with(LevelFilter::TRACE)
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_error::ErrorLayer::default())
        .try_init()?;

    let mut connection = Connection::create("127.0.0.1:1883", Duration::from_secs(10))?;

    let keep_alive = KeepAlive::try_from(Duration::from_secs(30))?;

    let connect = ConnectBuilder::create("mcutt-sync-sender")?
        .clean_session()
        .keepalive(keep_alive);

    let connack = connection
        .connect(connect)
        .and_then(|c| c.error_for_code())?;

    debug!(%connack);

    let instant = Instant::now();

    loop {
        let value = instant.elapsed().as_secs().to_be_bytes();

        let publish = PublishBuilder::new("interval/seconds", value.as_slice());

        connection.publish(&publish)?;

        std::thread::sleep(Duration::from_secs(1));
    }
}
