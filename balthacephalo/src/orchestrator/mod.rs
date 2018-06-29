mod manager;

use std::io;
use std::net::TcpStream;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

#[derive(Debug)]
pub enum Error {
    // ListenerRecvError(mpsc::RecvError), // Should only happen when the other end is disconnected
    ManagerError(manager::Error),
    IoError(io::Error),
    ThreadPanicked,
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
    let podes: Vec<Option<manager::Manager>> = Vec::new();
    let podes_rc = Arc::new(Mutex::new(podes));

    let (man_tx, man_rx) = mpsc::channel();

    /*let manager_creator_handle = */
    new_manager_creator(podes_rc.clone(), listener_rx, man_tx);

    for msg in man_rx.iter() {
        if let manager::Message::Disconnected(id) = msg {
            println!("Manager {} announced disconnected : Cleaning...", id);
            podes_rc.lock().unwrap()[id] = None;
        }
    }

    // TODO: Do I need to join the thread ? (possible problems with the mutex (use of a Weak ?) ?)
    // match manager_creator_handle.join() {
    //     Err(_) => return Err(Error::ThreadPanicked),
    //     Ok(Err(err)) => return Err(Error::from(err)),
    //     _ => (),
    // };

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

fn new_manager_creator(
    podes_rc: Arc<Mutex<Vec<Option<manager::Manager>>>>,
    listener_rx: mpsc::Receiver<TcpStream>,
    man_tx: mpsc::Sender<manager::Message>,
) -> thread::JoinHandle<Result<(), Error>> {
    thread::spawn(move || -> Result<(), Error> {
        for stream in listener_rx.iter() {
            let mut podes = podes_rc.lock().unwrap();

            let id = get_new_id(&mut podes);

            let mut manager = manager::Manager::new(id, stream, man_tx.clone());
            manager.manage()?;

            podes[id] = Some(manager);
        }

        println!("Channel from listener closed. Exiting...");

        Ok(())
    })
}