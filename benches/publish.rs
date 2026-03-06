use std::hint::black_box;
use std::net::TcpStream;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use mcutt::sync::Connection;
use mcutt::v3::packets::connect::KeepAlive;
use mcutt::v3::packets::connect::builder::ConnectBuilder;

fn connect() -> eyre::Result<Connection<TcpStream, TcpStream>> {
    let connection = TcpStream::connect("127.0.0.1:1883")?;
    connection.set_nodelay(true)?;
    connection.set_read_timeout(Some(Duration::from_secs(10)))?;
    connection.set_write_timeout(Some(Duration::from_secs(10)))?;

    let mut connection = Connection::new(connection.try_clone()?, connection);

    let keep_alive = KeepAlive::try_from(Duration::from_secs(0))?;

    let connect = ConnectBuilder::create("mcutt-bench")?
        .clean_session()
        .keepalive(keep_alive);

    let _connack = connection
        .connect(connect)
        .and_then(|c| c.error_for_code())?;

    Ok(connection)
}

pub fn publish(c: &mut Criterion) {
    let mut connect = connect().unwrap();

    c.bench_function("publish packet", |b| {
        b.iter(|| connect.publish(black_box("topic"), black_box(&[42])))
    });
}

criterion_group!(benches, publish);
criterion_main!(benches);
