use std::{net::TcpStream, time::Duration};

use mcutt::sync::Connection;
use mcutt::v3::packets::connect::KeepAlive;
use mcutt::v3::packets::connect::builder::ConnectBuilder;
use tracing::debug;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    tracing_subscriber::registry()
        .with(LevelFilter::TRACE)
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    let connection = TcpStream::connect("127.0.0.1:1883")?;
    connection.set_nodelay(true)?;
    connection.set_read_timeout(Some(Duration::from_secs(10)))?;
    connection.set_write_timeout(Some(Duration::from_secs(10)))?;

    let mut connection = Connection::new(connection.try_clone()?, connection);

    let keep_alive = KeepAlive::try_from(Duration::from_secs(10))?;

    let connect = ConnectBuilder::create("mcutt-sync-receiver")?
        .clean_session()
        .keepalive(keep_alive);

    let connack = connection
        .connect(connect)
        .and_then(|c| c.error_for_code())?;

    debug!(%connack);

    connection.subscribe("interval/seconds")?;

    loop {
        connection.recv()?;
    }
}
