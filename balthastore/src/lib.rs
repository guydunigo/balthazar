//! This crate contains different utilities to use different storage technologies
//! interchangeably and transparently.
//!
//!
//! See for instance the [`FetchStorage`] and [`StoreStorage`] traits used to create
//! a uniform way to manipulate different storage mechanisms.
//!
//!
//! This crate provides a wrapper structure to use the different Storages in a transparent way, see
//! [`wrapper::StoragesWrapper`].
//!
//!
//! As of now, only a storage for [Ipfs](https://ipfs.io) is implemented through
//! [`ipfs::IpfsStorage`].
extern crate bytes;
extern crate futures;
extern crate parity_multiaddr as multiaddr;

use bytes::Bytes;
use futures::{future::BoxFuture, stream::BoxStream, FutureExt, StreamExt};
use std::{error::Error, io};

mod config;
pub mod ipfs;
mod multiaddr_tools;
mod wrapper;

pub use config::{StorageConfig, StorageType};

pub use multiaddr::Multiaddr;
pub use multiaddr_tools::{
    try_internet_multiaddr_to_usual_format, MultiaddrToStringConversionError,
};
pub use wrapper::*;

// TODO: That's a lot of boxes everywhere for Storage trait and GenericReader...

/// This trait defines a generic interface for storage classes to fetch content.
pub trait FetchStorage: Sync {
    /// Get the requested data from the Storage as a [`futures::Stream`].
    /// > To be noted that the stream shouldn't eagerly download too much data,
    /// > so if it is dropped, the connection should be cut and the un-polled data dropped.
    fn fetch_stream(&self, addr: &str) -> BoxStream<Result<Bytes, Box<dyn Error + Send>>>;

    /// Returns the size in bytes of the file at given address.
    fn get_size(&self, addr: &str) -> BoxFuture<Result<u64, Box<dyn Error + Send>>>;

    /// Same as [`FetchStorage::fetch_stream`] but to fetch all the data as once.
    /// To prevent the memory being filled by a oversized file,
    /// when more than `max_bytes` bytes have been downloaded, the connection should
    /// be stopped.
    fn fetch<'a>(
        &'a self,
        addr: &'a str,
        max_bytes: u64,
    ) -> BoxFuture<'a, Result<Bytes, Box<dyn Error + Send>>> {
        // TODO: not very efficient ?
        async move {
            let file_size = max_bytes; // self.get_size(addr).await?;
            let mut downloaded_size: u64 = 0;
            // TODO: file_size or max_bytes ?
            let mut tmp = Vec::with_capacity(file_size as usize);
            let mut stream = self.fetch_stream(addr);
            while let Some(chunk_res) = stream.next().await {
                match chunk_res {
                    Ok(chunk) => {
                        tmp.extend_from_slice(&chunk[..]);
                        // If we downloaded too much data, stop and return what has been
                        // downloaded up to now.
                        downloaded_size += chunk.len() as u64;
                        if downloaded_size > max_bytes {
                            break;
                        }
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(Bytes::from(tmp))
        }
        .boxed()
    }
}

/// This trait defines a storage which allow receiving data.
/// As it's useless storing if you can't fetch, a StoreStorage should also
/// implement FetchStorage, and the address returned by a store operation
/// should return the exact same data when passed to fetch.
pub trait StoreStorage: FetchStorage {
    /// Stores provided data from the Storage coming from an async stream.
    /// TODO: blocking io::Read required by IPFS
    fn store_stream(
        &self,
        data_stream: GenericReader,
    ) -> BoxFuture<Result<String, Box<dyn Error + Send>>>;
    /// Same as [`StoreStorage::store_stream`] but to provide all the data as once.
    fn store(&self, data: &[u8]) -> BoxFuture<Result<String, Box<dyn Error + Send>>> {
        // TODO: ugly? needed to avoid static lifetime on data...
        // let vec = Vec::from(data);
        // let mut cursor = io::Cursor::new(vec);

        self.store_stream(data.into())
    }
}

// TODO: try with a simple `type GenericReader=Box<dyn io::Read + Send + Sync>`
/// This structure helps circumvent the problems arising with Generic types in [`FetchStorage`] trait.
/// Indeed, the error `the trait cannot be made into an object` is caused by using directly a
/// generic type.
pub struct GenericReader {
    inner: Box<dyn io::Read + Send + Sync>,
}

impl GenericReader {
    pub fn new<T: 'static + io::Read + Send + Sync>(inner: T) -> Self {
        GenericReader {
            inner: Box::new(inner),
        }
    }
}

impl From<&[u8]> for GenericReader {
    fn from(src: &[u8]) -> Self {
        // TODO: ugly? needed to avoid static lifetime on data...
        let vec = Vec::from(src);
        let cursor = io::Cursor::new(vec);

        GenericReader {
            inner: Box::new(cursor),
        }
    }
}

impl io::Read for GenericReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

#[cfg(test)]
mod tests {
    /// Only for testing the different storage:
    /// Place here the files you want to try to store or compare.
    pub const TEST_DIR: &str = "./test_files";
}
