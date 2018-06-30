#[macro_use]
extern crate serde_derive;

extern crate ron;
extern crate serde;

pub use ron::{de, ser};

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Hello(String),
    Connected(usize),
    Disconnect,
    Disconnected(usize),
    Idle(usize),
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
