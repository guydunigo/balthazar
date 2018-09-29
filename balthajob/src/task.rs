use std::sync::Weak;

// TODO: id with clone ?
#[derive(Debug, Clone)]
pub struct Task<T> {
    pub id: usize,
    pub args: Vec<u8>, // TODO: wasm arg list ?
    pub result: Option<Result<Vec<u8>, ()>>,
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
        self.result.is_none() && match &self.pode {
            None => true,
            Some(p) => p.upgrade().is_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Task;
    use std::sync::Arc;

    #[test]
    fn available_when_created() {
        let t: Task<u8> = Task::new(1, Vec::new());
        assert!(t.is_available());
    }
    #[test]
    fn not_available_if_result() {
        let mut t: Task<u8> = Task::new(1, Vec::new());
        t.result = Some(Ok(Vec::new()));
        assert!(!t.is_available());
    }
    #[test]
    fn not_available_if_attributed() {
        let mut t: Task<u8> = Task::new(1, Vec::new());
        let arc = Arc::new(0);
        let weak = Arc::downgrade(&arc);
        t.pode = Some(weak);

        assert!(!t.is_available());
    }
    #[test]
    fn available_if_attributed_is_dead() {
        let mut t: Task<u8> = Task::new(1, Vec::new());
        let arc = Arc::new(0);
        let weak = Arc::downgrade(&arc);
        t.pode = Some(weak);

        drop(arc);

        assert!(t.is_available());
    }
}
