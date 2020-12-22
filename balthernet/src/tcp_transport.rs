//! This is an example tcp-based transport. It is maybe too simple and supports probably
//! too few communication models to be used directly.
use futures::executor::block_on;
use futures::{io::AsyncRead, io::AsyncWrite, AsyncReadExt, AsyncWriteExt, TryStreamExt};
use libp2p::{
    core::transport::upgrade::Version, identity::Keypair, noise::NoiseConfig, tcp::TcpConfig,
    yamux::YamuxConfig, Multiaddr, Transport,
    noise,
};
use std::marker::Unpin;

async fn client(addr: Multiaddr, transport: impl Transport<Output = impl AsyncRead + Unpin>) {
    let mut buffer = String::new();
    let mut conn = transport.dial(addr).unwrap().await.unwrap();
    conn.read_to_string(&mut buffer).await.unwrap();
    println!("Received: {}", buffer.len());
}

async fn listener(
    addr: Multiaddr,
    transport: impl Transport<Output = impl AsyncWrite + Unpin>,
    port: u16,
) {
    println!("Listening on {}", port);
    transport
        .listen_on(addr)
        .unwrap()
        .try_for_each(|evt| async move {
            println!("new client");
            if let Some((stream_fut, _)) = evt.into_upgrade() {
                let mut stream = stream_fut.await.unwrap();
                stream.write_all(&[65, 65, 65, b'\n']).await.unwrap();
                println!("sent");
            } else {
                println!("no evts");
            }
            Ok(())
        })
        .await
        .unwrap()
}

/// Creates a tcp listener listening on `addr` if `listening_port` is `Some`, or a client which
/// connects to `addr`.
pub fn create_basic_tcp_client_or_listener(addr: Multiaddr, listening_port: Option<u16>) {
    let transport = TcpConfig::new();

    if let Some(listening_port) = listening_port {
        let fut = listener(addr, transport, listening_port);
        block_on(fut);
    } else {
        let fut = client(addr, transport);
        block_on(fut);
    }
}

/// Creates a [`Transport`] using only tcp and noise for encryption and yamux for multiplexing,
/// to be used in a [`Swarm`](`libp2p::swarm::Swarm`).
pub fn get_tcp_transport<E, L, LU, D>(keypair: Keypair) -> impl Transport {
    let dh_keys = noise::Keypair::<noise::X25519Spec>::new().into_authentic(&keypair).unwrap();
    let noise = NoiseConfig::xx(dh_keys).into_authenticated();
    let yamux = YamuxConfig::default();
    TcpConfig::new()
        .upgrade(Version::V1Lazy)
        .authenticate(noise)
        .multiplex(yamux)
}
