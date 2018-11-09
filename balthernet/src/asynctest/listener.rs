use tokio::codec::Framed;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::net::SocketAddr;

// TODO: local Error
use super::{Error, MessageCodec};
use balthmessage::Message;

pub fn for_each_message(addr: SocketAddr, socket: TcpStream, msg: &Message) -> Result<(), Error> {
    match msg {
        Message::Ping => {
            let framed_sock = Framed::new(socket, MessageCodec::new());
            let send_future = framed_sock
                .send(Message::Pong)
                .map(|_| ())
                .map_err(move |err| eprintln!("{} : Could no send `Pong` : {:?}", addr, err));

            tokio::spawn(send_future);
        }
        _ => println!("{} : received a message !", addr),
    }
    Ok(())
}

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

            // TODO: unwrap?
            let manager = framed_sock
                .map_err(Error::from)
                .for_each(move |msg| for_each_message(addr, socket.try_clone().unwrap(), &msg))
                .map_err(move |err| {
                    eprintln!("{} : error when receiving a message : {:?}.", addr, err)
                });

            tokio::spawn(manager);

            Ok(())
        })
        .map_err(|err| eprintln!("{:?}", err))
}
