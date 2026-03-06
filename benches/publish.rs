use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use mcutt::sync::Connection;
use mcutt::v3::packets::connect::KeepAlive;
use mcutt::v3::packets::connect::builder::ConnectBuilder;
use mcutt::v3::packets::publish::builder::PublishBuilder;

fn connect() -> eyre::Result<Connection<mio::net::TcpStream>> {
    let mut connection = Connection::create("127.0.0.1:1883", Duration::from_secs(10))?;

    let keep_alive = KeepAlive::try_from(Duration::from_secs(30))?;

    let connect = ConnectBuilder::create("mcutt-bench")?
        .clean_session()
        .keepalive(keep_alive);

    let _connack = connection
        .connect(connect)
        .and_then(|c| c.error_for_code())?;

    Ok(connection)
}

pub fn publish(c: &mut Criterion) {
    let connect = connect().unwrap();

    let publish = PublishBuilder::new("topic", &[42]);

    c.bench_function("publish packet", |b| {
        b.iter(|| connect.publish(black_box(&publish)))
    });
}

criterion_group!(benches, publish);
criterion_main!(benches);
