extern crate bytes;
extern crate futures;
extern crate multiaddr;

use bytes::Bytes;
use futures::{future::BoxFuture, io::AsyncRead, stream::BoxStream, FutureExt, StreamExt};
use std::{error::Error, io};

pub mod ipfs;

pub type FileAddr = String;

/// This trait defines a generic interface for storage mechanisms so they can be used interchangeably.
pub trait Storage: Sync {
    type Error: Error;

    /// Stores provided data from the Storage coming from an async stream.
    fn store_stream<T: 'static + io::Read + AsyncRead + Send + Sync>(
        &self,
        data_stream: T,
    ) -> BoxFuture<Result<FileAddr, Self::Error>>;
    /// Get the requested data from the Storage as a [`futures::Stream`]
    fn get_stream(&self, addr: &FileAddr) -> BoxStream<Result<Bytes, Self::Error>>;

    /// Same as [`Storage::store_stream`] but to provide all the data as once.
    fn store(&self, data: &'static [u8]) -> BoxFuture<Result<FileAddr, Self::Error>> {
        self.store_stream(data)
    }

    /// Same as [`Storage::get_stream`] but to get all the data as once.
    fn get<'a>(&'a self, addr: &'a FileAddr) -> BoxFuture<'a, Result<Bytes, Self::Error>> {
        // TODO: not very efficient ?
        let mut tmp = Vec::new();
        async move {
            while let Some(chunk_res) = self.get_stream(addr).next().await {
                match chunk_res {
                    Ok(chunk) => tmp.extend_from_slice(&chunk[..]),
                    Err(error) => return Err(error),
                }
            }
            Ok(Bytes::from(tmp))
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
