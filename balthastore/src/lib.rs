//! This crate contains different utilities to use different storage technologies
//! interchangeably and transparently.
//!
//!
//! See for instance the [`Storage`] trait used to create a uniform way to manipulate
//! different storage mechanisms.
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
extern crate multiaddr;

use bytes::Bytes;
use futures::{future::BoxFuture, stream::BoxStream, FutureExt, StreamExt};
use std::{error::Error, io};

pub mod ipfs;
mod multiaddr_tools;
mod wrapper;

pub use multiaddr_tools::{
    try_internet_multiaddr_to_usual_format, MultiaddrToStringConversionError,
};
pub use wrapper::*;

// TODO: That's a lot of boxes everywhere for Storage trait and GenericReader...

// TODO: Multiaddr?
/// Value used to reference to a file on a [`Storage`].
pub type FileAddr = String;

/// This trait defines a generic interface for storage mechanisms so they can be used interchangeably.
pub trait Storage: Sync {
    /// Stores provided data from the Storage coming from an async stream.
    /// TODO: blocking io::Read required by
    fn store_stream(
        &self,
        data_stream: GenericReader,
    ) -> BoxFuture<Result<FileAddr, Box<dyn Error>>>;
    /// Get the requested data from the Storage as a [`futures::Stream`]
    fn get_stream(&self, addr: &FileAddr) -> BoxStream<Result<Bytes, Box<dyn Error>>>;

    /// Same as [`Storage::store_stream`] but to provide all the data as once.
    fn store(&self, data: &[u8]) -> BoxFuture<Result<FileAddr, Box<dyn Error>>> {
        // TODO: ugly? needed to avoid static lifetime on data...
        // let vec = Vec::from(data);
        // let mut cursor = io::Cursor::new(vec);

        self.store_stream(data.into())
    }

    /// Same as [`Storage::get_stream`] but to get all the data as once.
    fn get<'a>(&'a self, addr: &'a FileAddr) -> BoxFuture<'a, Result<Bytes, Box<dyn Error>>> {
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

impl<T: Storage> Storage for &T {
    fn store_stream(
        &self,
        data_stream: GenericReader,
    ) -> BoxFuture<Result<FileAddr, Box<dyn Error>>> {
        (*self).store_stream(data_stream)
    }

    fn get_stream(&self, addr: &FileAddr) -> BoxStream<Result<Bytes, Box<dyn Error>>> {
        (*self).get_stream(addr)
    }
}

// TODO: try with a simple `type GenericReader=Box<dyn io::Read + Send + Sync>`
/// This structure helps circumvent the problems arising with Generic types in [`Storage`] trait.
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
