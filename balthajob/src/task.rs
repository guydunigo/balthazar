use std::sync::Weak;

// TODO: id with clone ?
#[derive(Debug, Clone)]
pub struct Task {
    pub id: usize,
    pub args: Vec<u8>, // TODO: wasm arg list ?
    pub result: Vec<u8>,
    pub pode: Option<Weak<u8>>,
    // TODO: date?
}

impl Task {
    pub fn new(id: usize, args: Vec<u8>) -> Task {
        Task {
            id,
            args,
            result: Vec::new(),
            pode: None,
        }
    }
}
