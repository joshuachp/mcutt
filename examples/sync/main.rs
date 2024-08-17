use std::{net::TcpStream, time::Duration};

use mcutt::{
    sync::Connection,
    v3::{
        connect::{Connect, KeepAlive},
        header::Str,
    },
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    tracing_subscriber::registry()
        .with(LevelFilter::DEBUG)
        .with(tracing_subscriber::fmt::layer())
        .try_init()?;

    let connection = TcpStream::connect("127.0.0.1:1883")?;
    connection.set_read_timeout(Some(Duration::from_secs(10)))?;

    let mut connection = Connection::new(&connection);

    let keep_alive = KeepAlive::try_from(Duration::from_secs(10))?;
    let client_id = Str::try_from("mcutt-sync-client")?;

    let mut connect = Connect::new(client_id, keep_alive);

    connect.clean_session();

    let connack = connection.connect(&connect)?;

    println!("{connack}");

    Ok(())
}
