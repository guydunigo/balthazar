//! Provides [`IpfsStorage`] to use the [InterPlanetary File-System (IPFS)](https://ipfs.io)
extern crate either;
extern crate http;
extern crate ipfs_api;
extern crate tokio;

use bytes::Bytes;
use either::Either;
use futures::{future::BoxFuture, stream::BoxStream, FutureExt, StreamExt, TryStreamExt};
use http::uri::InvalidUri;
use ipfs_api::{response, IpfsClient};
use multiaddr::Multiaddr;
use std::{error::Error, fmt};

use super::{
    try_internet_multiaddr_to_usual_format, GenericReader, MultiaddrToStringConversionError,
    Storage,
};

/// Wrapper arround [`ipfs_api::response::Error`] to implement trait [`std::error::Error`].
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

/// Storage to use the [InterPlanetary File-System (IPFS)](https://ipfs.io)
///
/// Creating it through the [`Default::default`] trait connects to the default Ipfs
/// port on `localhost:5001`.
#[derive(Clone, Default)]
pub struct IpfsStorage {
    // TODO: For performance reasons, recreate the client each time ?
    ipfs_client: IpfsClient,
}

impl IpfsStorage {
    /// Creates a new client connecting to the listening multiaddr.
    pub fn new(
        listen_addr: Multiaddr,
    ) -> Result<Self, Either<InvalidUri, MultiaddrToStringConversionError>> {
        let usual_addr =
            try_internet_multiaddr_to_usual_format(&listen_addr).map_err(Either::Right)?;
        let http_addr = format!("http://{}", usual_addr);
        Ok(IpfsStorage {
            ipfs_client: IpfsClient::new_from_uri(&http_addr[..]).map_err(Either::Left)?,
        })
    }

    /// Get back the inner [`IpfsClient`] if a direct access is needed.
    pub fn into_inner(self) -> IpfsClient {
        self.ipfs_client
    }

    /// Returns the inner [`IpfsClient`] if a direct access is needed.
    pub fn inner(&self) -> &IpfsClient {
        &self.ipfs_client
    }
}

impl Storage for IpfsStorage {
    fn store_stream(
        &self,
        data_stream: GenericReader,
    ) -> BoxFuture<Result<Vec<u8>, Box<dyn Error>>> {
        let new_client = self.inner().clone();
        async move {
            let res = new_client.add(data_stream).await;
            match res {
                Ok(res) => Ok(format!("/ipfs/{}", res.name).into()),
                Err(error) => {
                    let error: Box<dyn Error> = Box::new(IpfsApiResponseError::from(error));
                    Err(error)
                }
            }
        }
        .boxed()
    }

    fn get_stream(&self, addr: &[u8]) -> BoxStream<Result<Bytes, Box<dyn Error>>> {
        self.ipfs_client
            .cat(&String::from_utf8_lossy(addr)[..])
            .map_err(|e| {
                let error: Box<dyn Error> = Box::new(IpfsApiResponseError::from(e));
                error
            })
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::TEST_DIR;
    use super::*;
    use std::fs;
    use tokio::runtime::Runtime;

    const TEST_FILE: &[u8] = b"/ipfs/QmPZ9gcCEpqKTo6aq61g2nXGUhM4iCL3ewB6LDXZCtioEB";

    fn get_test_file_name() -> String {
        format!("{}{}", TEST_DIR, String::from_utf8_lossy(TEST_FILE))
    }

    #[test]
    fn it_connects_to_given_address() {
        let storage = IpfsStorage::new("/dns4/ipfs.io".parse().unwrap()).unwrap();

        Runtime::new()
            .unwrap()
            .block_on(storage.get(&TEST_FILE[..]))
            .unwrap();
    }

    #[test]
    fn it_stores_a_correct_file_stream() {
        let storage = IpfsStorage::default();
        let file = fs::File::open(get_test_file_name()).unwrap();

        let res = Runtime::new()
            .unwrap()
            .block_on(storage.store_stream(GenericReader::new(file)))
            .unwrap();

        assert_eq!(TEST_FILE, &res[..]);
    }

    #[test]
    fn it_stores_a_correct_file() {
        let storage = IpfsStorage::default();
        let content = fs::read(get_test_file_name()).unwrap();

        let file_name = Runtime::new()
            .unwrap()
            .block_on(storage.store(&content[..]))
            .unwrap();

        assert_eq!(TEST_FILE, &file_name[..]);
    }

    #[test]
    fn it_reads_a_correct_file() {
        let storage = IpfsStorage::default();
        let content = fs::read(get_test_file_name()).unwrap();

        let data = Runtime::new()
            .unwrap()
            .block_on(storage.get(&TEST_FILE[..]))
            .unwrap();

        assert_eq!(content, data);
    }
}
