//! Provides [`StoragesWrapper`] to use different storages at once in a transparently.
// TODO: Instructions to add new Storage

use super::{ipfs, FetchStorage, GenericReader, StorageConfig, StorageType, StoreStorage};
use bytes::Bytes;
use futures::{future::BoxFuture, stream::BoxStream};
use multiaddr::{Multiaddr, Protocol};
use std::{convert::TryFrom, error::Error};

pub type StoragesWrapperCreationError = ipfs::IpfsStorageCreationError;

/// This structure is a wrapper around different storages to automatically route the calls to the
/// corresponding storage.
/// For instance, files named using the format `/ipfs/[MULTIHASH]` will be routed towards the
/// underlying [`ipfs::IpfsStorage`].
///
/// [`StoragesWrapper`] has a **default** storage defined and [`StoreStorage::store`] calls will use it and [`FetchStorage::fetch`] calls that couldn't be automatically linked to another storage are sent to it as well.
#[derive(Clone, Default)]
pub struct StoragesWrapper {
    config: StorageConfig,
    ipfs: ipfs::IpfsStorage,
}

impl StoragesWrapper {
    pub fn new_with_config(config: &StorageConfig) -> Result<Self, StoragesWrapperCreationError> {
        let ipfs = if let Some(addr) = config.ipfs_api() {
            ipfs::IpfsStorage::new(addr)?
        } else {
            Default::default()
        };

        Ok(StoragesWrapper {
            config: config.clone(),
            ipfs,
        })
    }

    pub fn storage_type_to_storage(&self, storage_type: &StorageType) -> &dyn StoreStorage {
        match storage_type {
            StorageType::Ipfs => self.storage_ipfs(),
        }
    }

    pub fn default_storage_type(&self) -> &StorageType {
        self.config.default_storage()
    }

    /// Returns the [`StoreStorage`] corresponding to the
    /// [`default_storage_type`](`StoragesWrapper::default_storage_type`) answer.
    pub fn default_storage(&self) -> &dyn StoreStorage {
        self.storage_type_to_storage(self.default_storage_type())
    }

    pub fn storage_ipfs(&self) -> &ipfs::IpfsStorage {
        &self.ipfs
    }

    /// Tries to determine the best storage to use to obtain `addr`.
    ///
    /// It uses these rules:
    /// - if `addr` is in the format `/ipfs/[MULTIHASH]`: StorageType::Ipfs
    /// - otherwise: [`default_storage_type`](`StoragesWrapper::default_storage_type`)
    ///
    /// ## For example:
    /// ```rust
    /// # use balthastore::{StoragesWrapper, StorageType};
    ///
    /// let wrapper = StoragesWrapper::default();
    /// let addr = "/ipfs/QmPZ9gcCEpqKTo6aq61g2nXGUhM4iCL3ewB6LDXZCtioEB";
    ///
    /// let storage_type = wrapper.get_storage_type_based_on_address(addr);
    ///
    /// assert_eq!(storage_type, StorageType::Ipfs);
    ///
    /// ```
    pub fn get_storage_type_based_on_address(&self, addr: &str) -> StorageType {
        match Multiaddr::try_from(addr.to_owned()) {
            Ok(multiaddr) => match multiaddr.iter().next() {
                Some(Protocol::P2p(_)) => StorageType::Ipfs,
                _ => *self.default_storage_type(),
            },
            Err(_) => *self.default_storage_type(),
        }
    }

    /// Returns the [`StoreStorage`] corresponding to the [`get_storage_type_based_on_address`](`StoragesWrapper::get_storage_type_based_on_address`) answer.
    pub fn get_storage_based_on_address(&self, addr: &str) -> &dyn StoreStorage {
        let storage_type = self.get_storage_type_based_on_address(addr);
        self.storage_type_to_storage(&storage_type)
    }
}

impl FetchStorage for StoragesWrapper {
    /// Tries to determine automatically the best [`StoreStorage`] to retrieve data at `addr`
    /// using [`get_storage_type_based_on_address`](`StoragesWrapper::get_storage_type_based_on_address`).
    fn fetch_stream(&self, addr: &str) -> BoxStream<Result<Bytes, Box<dyn Error + Send>>> {
        self.get_storage_based_on_address(addr).fetch_stream(addr)
    }

    fn get_size(&self, addr: &str) -> BoxFuture<Result<u64, Box<dyn Error + Send>>> {
        self.get_storage_based_on_address(addr).get_size(addr)
    }
}

impl StoreStorage for StoragesWrapper {
    /// Stores the data into the [`default_storage_type`](`StoragesWrapper::default_storage_type`).
    fn store_stream(
        &self,
        data_stream: GenericReader,
    ) -> BoxFuture<Result<String, Box<dyn Error + Send>>> {
        self.default_storage().store_stream(data_stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_correct_type_based_on_address(addr: &str, expected_type: StorageType) {
        let wrapper = StoragesWrapper::default();
        let storage_type = wrapper.get_storage_type_based_on_address(addr);
        assert_eq!(storage_type, expected_type);
    }

    #[test]
    fn it_chooses_the_correct_storage_type_based_on_address() {
        let addr = "/ipfs/QmPZ9gcCEpqKTo6aq61g2nXGUhM4iCL3ewB6LDXZCtioEB";
        check_correct_type_based_on_address(addr, StorageType::Ipfs);
    }
}
