pub mod ipfs;

pub type FileAddr = String;

pub trait Storage {
    type Error: Error;

    fn store(data: &[u8]) -> Result<FileAddr, Self::Error>;
    fn get(addr: &FileAddr) -> Result<Vec<u8>, Self::Error>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
