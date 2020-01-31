extern crate http;
extern crate ipfs_api;

use bytes::Bytes;
use futures::{
    future::BoxFuture, io::AsyncRead, stream::BoxStream, FutureExt, StreamExt, TryStreamExt,
};
use http::uri::InvalidUri;
use ipfs_api::response;
use ipfs_api::IpfsClient;
use multiaddr::Multiaddr;
use std::{error::Error, fmt, io};

use super::{FileAddr, Storage};

/// Wrapper arround [`ipfs_api::response::Error`] to implement trait [`std::error:Error`].
#[derive(Debug)]
pub struct IpfsApiResponseError {
    inner: response::Error,
}
impl fmt::Display for IpfsApiResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}
impl Error for IpfsApiResponseError {}
impl From<response::Error> for IpfsApiResponseError {
    fn from(src: response::Error) -> Self {
        IpfsApiResponseError { inner: src }
    }
}

/// Ipfs storage for use by the nodes.
/// Creating it through the [`Default::default`] trait connects to the default Ipfs
/// port on `localhost:5001`.
#[derive(Clone, Default)]
pub struct IpfsStorage {
    ipfs_client: IpfsClient,
}

impl IpfsStorage {
    /// Creates a new client connecting to the listening multiaddr.
    pub fn new(listen_addr: Multiaddr) -> Result<Self, InvalidUri> {
        Ok(IpfsStorage {
            ipfs_client: IpfsClient::new_from_uri(&listen_addr.to_string()[..])?,
        })
    }

    /// Return the inner [`IpfsClient`] if a direct access is needed.
    pub fn into_inner(&self) -> &IpfsClient {
        &self.ipfs_client
    }
}

impl Storage for IpfsStorage {
    type Error = IpfsApiResponseError;

    fn store_stream<T: 'static + io::Read + AsyncRead + Send + Sync>(
        &self,
        data_stream: T,
    ) -> BoxFuture<Result<FileAddr, Self::Error>> {
        async move {
            let res = self.ipfs_client.add(data_stream).await;
            println!("Stored on IPFS : {:?}", res);
            match res {
                Ok(res) => Ok(res.name),
                Err(error) => Err(error.into()),
            }
        }
        .boxed()
    }

    fn get_stream(&self, addr: &FileAddr) -> BoxStream<Result<Bytes, Self::Error>> {
        self.ipfs_client.get(addr).map_err(|e| e.into()).boxed()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
