use std::sync::Weak;

// TODO: id with clone ?
#[derive(Debug, Clone)]
pub struct Task<T> {
    pub id: usize,
    pub args: Vec<u8>, // TODO: wasm arg list ?
    pub result: Option<Vec<u8>>,
    pub pode: Option<Weak<T>>,
    // TODO: date?
}

impl<T> Task<T> {
    pub fn new(id: usize, args: Vec<u8>) -> Task<T> {
        Task {
            id,
            args,
            result: None,
            pode: None,
        }
    }

    pub fn is_available(&self) -> bool {
        self.result.is_none() && match self.pode {
            None => true,
            Some(p) => p.upgrade().is_none(),
        }
    }
}

/*
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
*/
