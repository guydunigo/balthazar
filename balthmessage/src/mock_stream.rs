use std::io;
use std::io::prelude::*;

pub struct MockStream {
    pub bytes: Vec<u8>,
}
impl MockStream {
    pub fn new() -> MockStream {
        MockStream { bytes: Vec::new() }
    }
}
impl Write for MockStream {
    fn write(&mut self, bytes: &[u8]) -> std::result::Result<usize, std::io::Error> {
        bytes.iter().for_each(|b| self.bytes.push(b.clone()));
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::result::Result<(), std::io::Error> {
        unimplemented!();
    }
}
impl Read for MockStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let buf_len = buf.len();
        let bytes_len = self.bytes.len();
        let length = if buf_len > bytes_len {
            bytes_len
        } else {
            buf_len
        };

        if length > 0 {
            let read_vec: Vec<u8> = self.bytes.drain(..length).collect();
            buf.copy_from_slice(&read_vec[..]);

            Ok(length)
        } else {
            Ok(0)
        }
    }
}
