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
    try_internet_multiaddr_to_usual_format, FetchStorage, GenericReader,
    MultiaddrToStringConversionError, StoreStorage,
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

pub type IpfsStorageCreationError = Either<InvalidUri, MultiaddrToStringConversionError>;

impl IpfsStorage {
    /// Creates a new client connecting to the listening multiaddr.
    pub fn new(listen_addr: &Multiaddr) -> Result<Self, IpfsStorageCreationError> {
        let usual_addr =
            try_internet_multiaddr_to_usual_format(listen_addr).map_err(Either::Right)?;
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

impl FetchStorage for IpfsStorage {
    fn fetch_stream(&self, addr: &[u8]) -> BoxStream<Result<Bytes, Box<dyn Error + Send>>> {
        self.ipfs_client
            .cat(&String::from_utf8_lossy(addr)[..])
            .map_err(|e| {
                let error: Box<dyn Error + Send> = Box::new(IpfsApiResponseError::from(e));
                error
            })
            .boxed()
    }

    // TODO: fetch downloads the file first, worst solution but only one found which returns
    // exactly correct size...
    // TODO: object_stat size isn't exact file size... how to do that without downloading data ?
    fn get_size(&self, addr: &[u8]) -> BoxFuture<Result<u64, Box<dyn Error + Send>>> {
        // TODO: utterly ugly and disgusting :'P
        /*
        let addr = String::from_utf8_lossy(addr).to_string();
        async move {
            self.ipfs_client
                .object_stat(&addr[..])
                .await
                .map(|s| {
                    eprintln!("{:?}", s);
                    // TODO: usize
                    s.cumulative_size
                })
                .map_err(|e| {
                    let error: Box<dyn Error + Send> = Box::new(IpfsApiResponseError::from(e));
                    error
                })
        }.boxed()
        */
        let addr = Vec::from(addr);
        async move {
            self.fetch(&addr[..], 1_000_000)
                .await
                .map(|d| d.len() as u64)
        }
        .boxed()
    }
}

impl StoreStorage for IpfsStorage {
    fn store_stream(
        &self,
        data_stream: GenericReader,
    ) -> BoxFuture<Result<Vec<u8>, Box<dyn Error + Send>>> {
        let new_client = self.inner().clone();
        async move {
            let res = new_client.add(data_stream).await;
            match res {
                Ok(res) => Ok(format!("/ipfs/{}", res.name).into()),
                Err(error) => {
                    let error: Box<dyn Error + Send> = Box::new(IpfsApiResponseError::from(error));
                    Err(error)
                }
            }
        }
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
    // const TEST_FILE_2: &[u8] = b"/ipfs/QmXfbZ7H946MeecTWZqcdWKnPwudcqcokTFctJ5LeqMDK3";

    fn get_test_file_name() -> String {
        format!("{}{}", TEST_DIR, String::from_utf8_lossy(TEST_FILE))
    }

    #[test]
    fn it_connects_to_given_address() {
        let storage = IpfsStorage::new(&"/dns4/ipfs.io".parse().unwrap()).unwrap();

        Runtime::new()
            .unwrap()
            .block_on(storage.fetch(&TEST_FILE[..], 1_000_000))
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
            .block_on(storage.fetch(TEST_FILE, 1_000_000))
            .unwrap();

        assert_eq!(content, data);
    }

    #[test]
    fn it_reads_a_correct_file_size() {
        let storage = IpfsStorage::default();
        let expected_size: u64 = std::fs::metadata(get_test_file_name()).unwrap().len();
        // let expected_size: u64 = Runtime::new().unwrap().block_on(storage.fetch(TEST_FILE_2, 1_000_000)).unwrap().len() as u64;

        let size = Runtime::new()
            .unwrap()
            .block_on(storage.get_size(TEST_FILE))
            .unwrap();

        assert_eq!(expected_size, size);
    }
}
