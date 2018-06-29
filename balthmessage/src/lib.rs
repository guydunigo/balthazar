#[macro_use]
extern crate serde_derive;

extern crate serde;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Hello(String),
    Bye(u8),
    Connected(usize),
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
