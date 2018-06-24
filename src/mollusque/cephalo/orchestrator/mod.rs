mod manager;

use std::io;
use std::net::TcpStream;
use std::sync::mpsc;
// use std::thread;

#[derive(Debug)]
pub enum Error {
    // ListenerRecvError(mpsc::RecvError), // Should only happen when the other end is disconnected
    ManagerError(manager::Error),
    IoError(io::Error),
}

impl From<manager::Error> for Error {
    fn from(err: manager::Error) -> Error {
        Error::ManagerError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

pub fn orchestrate(listener_rx: mpsc::Receiver<TcpStream>) -> Result<(), Error> {
    let mut podes: Vec<Option<manager::Manager>> = Vec::new();
    let (man_tx, man_rx) = mpsc::channel();

    for stream in listener_rx.iter() {
        let id = get_new_id(&mut podes);
        let mut manager = manager::Manager::new(id, stream, man_tx.clone());
        manager.manage()?;
        podes[id] = Some(manager);
    }

    println!("Channel from listener closed. Exiting...");

    Ok(())
}

fn get_new_id<T>(vec: &mut Vec<Option<T>>) -> usize {
    for elm in vec.iter().enumerate() {
        if elm.1.is_none() {
            return elm.0;
        }
    }

    vec.push(None);
    vec.len() - 1
}
