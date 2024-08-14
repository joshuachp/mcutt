use std::{
    error::Error,
    io::{BufWriter, Read},
    net::TcpStream,
    time::Duration,
};

use mcutt::{
    sync::TcpClient,
    v3::{
        connect::{Connect, KeepAlive},
        header::Str,
    },
};

type DynError = Box<dyn Error + Send + Sync + 'static>;

fn main() -> Result<(), DynError> {
    let connection = TcpStream::connect("127.0.0.1:1883")?;
    connection.set_read_timeout(Some(Duration::from_secs(10)))?;

    let writer = BufWriter::new(&connection);

    let mut client = TcpClient::new(writer);

    let keep_alive = KeepAlive::try_from(Duration::from_secs(10))?;
    let client_id = Str::try_from("mcutt-sync-client")?;

    let mut connect = Connect::new(client_id, keep_alive);

    connect.clean_session();

    client.connect(connect)?;

    let mut buf = [0u8; 4];

    (&connection).read_exact(buf.as_mut_slice())?;

    println!("CONNACK:");
    for b in buf {
        println!("{b:08b}")
    }

    Ok(())
}
