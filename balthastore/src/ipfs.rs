//! Provides [`IpfsStorage`] to use the [InterPlanetary File-System (IPFS)](https://ipfs.io)
extern crate ipfs_api_backend_hyper as ipfs_api;

use bytes::Bytes;
use futures::{future::BoxFuture, stream::BoxStream, FutureExt, StreamExt, TryStreamExt};
use ipfs_api::{IpfsClient, TryFromUri, IpfsApi};
use multiaddr::Multiaddr;
use std::error::Error;

/// Error that can happen when the storage is created.
/// For instance, there will be an error if the IPFS server address provided is incorrect.
///
/// For now, this is just an alias for future-proofing in case I want to do more things
/// during creation like testing connectivity.
pub type IpfsStorageCreationError=multiaddr::Error;

use super::{
    FetchStorage, GenericReader, StoreStorage,
};

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
    pub fn new(listen_addr: &Multiaddr) -> Result<Self, IpfsStorageCreationError> {
        Ok(IpfsStorage {
            ipfs_client: IpfsClient::from_multiaddr(listen_addr.clone())?,
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
    fn fetch_stream(&self, addr: &str) -> BoxStream<Result<Bytes, Box<dyn Error + Send>>> {
        self.ipfs_client
            .cat(addr)
            .map_err(|e| {
                let error: Box<dyn Error + Send> = Box::new(e);
                error
            })
            .boxed()
    }

    // TODO: fetch downloads the file first, worst solution but only one found which returns
    // exactly correct size...
    // TODO: object_stat size isn't exact file size... how to do that without downloading data ?
    fn get_size(&self, addr: &str) -> BoxFuture<Result<u64, Box<dyn Error + Send>>> {
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
        let addr = String::from(addr);
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
    ) -> BoxFuture<Result<String, Box<dyn Error + Send>>> {
        let new_client = self.inner().clone();
        async move {
            let res = new_client.add(data_stream).await;
            match res {
                Ok(res) => Ok(format!("/ipfs/{}", res.name)),
                Err(error) => {
                    let error: Box<dyn Error + Send> = Box::new(error);
                    Err(error)
                }
            }
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    extern crate tokio;

    use super::super::tests::TEST_DIR;
    use super::*;
    use std::fs;

    const TEST_FILE: &str = "/ipfs/QmPZ9gcCEpqKTo6aq61g2nXGUhM4iCL3ewB6LDXZCtioEB";
    // const TEST_FILE_2: &str = b"/ipfs/QmXfbZ7H946MeecTWZqcdWKnPwudcqcokTFctJ5LeqMDK3";

    fn get_test_file_name() -> String {
        format!("{}{}", TEST_DIR, TEST_FILE)
    }

    #[tokio::test]
    async fn it_connects_to_given_address() {
        let storage = IpfsStorage::new(&"/dns4/ipfs.io/tcp/80".parse().unwrap()).unwrap();
        storage.fetch(TEST_FILE, 1_000_000).await.unwrap();
    }

    #[tokio::test]
    async fn it_stores_a_correct_file_stream() {
        let storage = IpfsStorage::default();
        let file = fs::File::open(get_test_file_name()).unwrap();

        let res = storage
            .store_stream(GenericReader::new(file))
            .await
            .unwrap();

        assert_eq!(TEST_FILE, &res[..]);
    }

    #[tokio::test]
    async fn it_stores_a_correct_file() {
        let storage = IpfsStorage::default();
        let content = fs::read(get_test_file_name()).unwrap();

        let file_name = storage.store(&content[..]).await.unwrap();

        assert_eq!(TEST_FILE, &file_name[..]);
    }

    #[tokio::test]
    async fn it_reads_a_correct_file() {
        let storage = IpfsStorage::default();
        let content = fs::read(get_test_file_name()).unwrap();

        let data = storage.fetch(TEST_FILE, 1_000_000).await.unwrap();

        assert_eq!(content, data);
    }

    #[tokio::test]
    async fn it_reads_a_correct_file_size() {
        let storage = IpfsStorage::default();
        let expected_size: u64 = std::fs::metadata(get_test_file_name()).unwrap().len();
        // let expected_size: u64 = Runtime::new().unwrap().block_on(storage.fetch(TEST_FILE_2, 1_000_000)).unwrap().len() as u64;

        let size = storage.get_size(TEST_FILE).await.unwrap();

        assert_eq!(expected_size, size);
    }
}
