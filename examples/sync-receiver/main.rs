use std::num::NonZero;
use std::time::Duration;

use mcutt::sync::{Connection, Receiver, Sender};
use mcutt::v3::packets::connect::KeepAlive;
use mcutt::v3::packets::connect::builder::ConnectBuilder;
use mcutt::v3::packets::subscribe::builder::{SubscribeBuilder, SubscribeFilter};
use mcutt::v3::packets::subscribe::topic::RequestedQos;
use tracing::debug;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    tracing_subscriber::registry()
        .with(LevelFilter::TRACE)
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    let mut connection = Connection::create("127.0.0.1:1883", Duration::from_secs(10))?;

    let keep_alive = KeepAlive::try_from(Duration::from_secs(30))?;

    let connect = ConnectBuilder::create("mcutt-sync-receiver")?
        .clean_session()
        .keepalive(keep_alive);

    let connack = connection
        .connect(connect)
        .and_then(|c| c.error_for_code())?;

    debug!(%connack);

    const PKID: NonZero<u16> = NonZero::new(1).unwrap();

    connection.subscribe(&SubscribeBuilder::with_topic(
        PKID,
        &SubscribeFilter {
            topic: "interval/seconds",
            qos: RequestedQos::AtMostOnce,
        },
    ))?;

    loop {
        connection.recv()?;
    }
}
