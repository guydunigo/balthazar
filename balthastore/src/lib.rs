extern crate bytes;
extern crate futures;
extern crate multiaddr;

use bytes::Bytes;
use futures::{future::BoxFuture, stream::BoxStream, FutureExt, StreamExt};
use std::{error::Error, io};

pub mod ipfs;

//TODO: Multiaddr
pub type FileAddr = String;

/// This trait defines a generic interface for storage mechanisms so they can be used interchangeably.
pub trait Storage: Sync {
    type Error: Error;

    /// Stores provided data from the Storage coming from an async stream.
    /// TODO: blocking io::Read required by
    fn store_stream<T: 'static + io::Read + Send + Sync>(
        &self,
        data_stream: T,
    ) -> BoxFuture<Result<FileAddr, Self::Error>>;
    /// Get the requested data from the Storage as a [`futures::Stream`]
    fn get_stream(&self, addr: &FileAddr) -> BoxStream<Result<Bytes, Self::Error>>;

    /// Same as [`Storage::store_stream`] but to provide all the data as once.
    fn store(&self, data: &[u8]) -> BoxFuture<Result<FileAddr, Self::Error>> {
        // TODO: ugly? needed to avoid static lifetime on data...
        let vec = Vec::from(data);
        let cursor = io::Cursor::new(vec);

        self.store_stream(cursor)
    }

    /// Same as [`Storage::get_stream`] but to get all the data as once.
    fn get<'a>(&'a self, addr: &'a FileAddr) -> BoxFuture<'a, Result<Bytes, Self::Error>> {
        // TODO: not very efficient ?
        async move {
            let mut tmp = Vec::new();
            let mut stream = self.get_stream(addr);
            while let Some(chunk_res) = stream.next().await {
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
