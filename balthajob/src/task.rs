use std::sync::Weak;

// TODO: id with clone ?
#[derive(Debug, Clone)]
pub struct Task<T> {
    pub id: usize,
    pub args: Vec<u8>, // TODO: wasm arg list ?
    pub result: Vec<u8>,
    pub pode: Option<Weak<T>>,
    // TODO: date?
}

impl<T> Task<T> {
    pub fn new(id: usize, args: Vec<u8>) -> Task<T> {
        Task {
            id,
            args,
            result: Vec::new(),
            pode: None,
        }
    }
}
