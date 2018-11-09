use tokio::codec::Framed;
use tokio::io;
use tokio::net::TcpListener;
use tokio::prelude::*;

use std::net::SocketAddr;

use super::MessageCodec;
use balthmessage::Message;

pub fn bind(local_addr: &SocketAddr) -> Result<TcpListener, io::Error> {
    TcpListener::bind(local_addr)
}

pub fn listen(listener: TcpListener) -> impl Future<Item = (), Error = ()> {
    listener
        .incoming()
        .for_each(|socket| {
            let addr = socket.peer_addr()?;
            println!("Asked for connection : `{}`", addr);

            let framed_sock = Framed::new(socket.try_clone()?, MessageCodec::new());

            let manager = framed_sock
                .for_each(move |msg| {
                    // TODO: unwrap?
                    let socket = socket.try_clone().unwrap();
                    match msg {
                        Message::Ping => {
                            let framed_sock = Framed::new(socket, MessageCodec::new());
                            let send_future =
                                framed_sock
                                    .send(Message::Pong)
                                    .map(|_| ())
                                    .map_err(move |err| {
                                        eprintln!("{} : Could no send `Pong` : {:?}", addr, err)
                                    });

                            tokio::spawn(send_future);
                        }
                        _ => println!("{} : received a message !", addr),
                    }
                    Ok(())
                })
                .map_err(move |err| {
                    eprintln!("{} : error when receiving a message : {:?}.", addr, err)
                });

            tokio::spawn(manager);

            Ok(())
        })
        .map_err(|err| eprintln!("{:?}", err))
}
