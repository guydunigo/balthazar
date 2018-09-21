use std::rc::Weak;

#[derive(Debug)]
pub struct Task {
    id: usize,
    args: Vec<u8>, // TODO: wasm arg list ?
    result: Vec<u8>,
    pode: Option<Weak<u8>>,
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
