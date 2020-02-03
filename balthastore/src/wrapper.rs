//! Provides [`StoragesWrapper`] to use different [`Storage`] at once in a transparently.
// TODO: Instructions to add new Storage

use super::{ipfs, FileAddr, GenericReader, MultiaddrToStringConversionError, Storage};
use bytes::Bytes;
use either::Either;
use futures::{future::BoxFuture, stream::BoxStream};
use http::uri::InvalidUri;
use multiaddr::{AddrComponent, Multiaddr};
use std::{error::Error, str::FromStr};

/// This enum defines the different [`StorageType`] available for [`StoragesWrapper`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StorageType {
    Ipfs,
    // TODO: Other(T) ?
}

impl Default for StorageType {
    fn default() -> Self {
        StorageType::Ipfs
    }
}

/// This structure is a wrapper around different storages to automatically route the calls to the
/// corresponding storage.
/// For instance, files named using the format `/ipfs/[MULTIHASH]` will be routed towards the
/// underlying [`ipfs::IpfsStorage`].
///
/// [`StoragesWrapper`] has a **default** storage defined and [`Storage::store`] calls will use it and [`Storage::get`] calls that couldn't be automatically linked to another [`Storage`] are sent to it as well.
#[derive(Clone, Default)]
pub struct StoragesWrapper {
    default_storage_type: StorageType,
    ipfs: ipfs::IpfsStorage,
}

impl StoragesWrapper {
    pub fn storage_type_to_storage(&self, storage_type: StorageType) -> &dyn Storage {
        match storage_type {
            StorageType::Ipfs => self.storage_ipfs(),
        }
    }

    pub fn default_storage_type(&self) -> StorageType {
        self.default_storage_type
    }

    /// Returns the [`Storage`] corresponding to the
    /// [`default_storage_type`](`StoragesWrapper::default_storage_type`) answer.
    pub fn default_storage(&self) -> &dyn Storage {
        self.storage_type_to_storage(self.default_storage_type())
    }

    pub fn set_default_storage_type(&mut self, default_storage_type: StorageType) {
        self.default_storage_type = default_storage_type;
    }

    pub fn storage_ipfs(&self) -> &ipfs::IpfsStorage {
        &self.ipfs
    }

    pub fn set_ipfs_listen_addr(
        &mut self,
        listen_addr: Multiaddr,
    ) -> Result<(), Either<InvalidUri, MultiaddrToStringConversionError>> {
        self.ipfs = ipfs::IpfsStorage::new(listen_addr)?;
        Ok(())
    }

    /// Tries to determine the best storage to use to obtain `addr`.
    ///
    /// It uses these rules:
    /// - if `addr` is in the format `/ipfs/[MULTIHASH]`: StorageType::Ipfs
    /// - otherwise: [`default_storage_type`](`StoragesWrapper::default_storage_type`)
    ///
    /// ## For example:
    /// ```rust
    /// # use balthastore::wrapper::{StoragesWrapper, StorageType};
    ///
    /// let wrapper = StoragesWrapper::default();
    /// let addr = "/ipfs/QmPZ9gcCEpqKTo6aq61g2nXGUhM4iCL3ewB6LDXZCtioEB".to_string();
    ///
    /// let storage_type = wrapper.get_storage_type_based_on_address(&addr);
    ///
    /// assert_eq!(storage_type, StorageType::Ipfs);
    ///
    /// ```
    pub fn get_storage_type_based_on_address(&self, addr: &FileAddr) -> StorageType {
        match Multiaddr::from_str(addr) {
            Ok(multiaddr) => match multiaddr.iter().next() {
                Some(AddrComponent::IPFS(_)) => StorageType::Ipfs,
                _ => self.default_storage_type(),
            },
            Err(_) => self.default_storage_type(),
        }
    }

    /// Returns the [`Storage`] corresponding to the [`get_storage_type_based_on_address`](`StoragesWrapper::get_storage_type_based_on_address`) answer.
    pub fn get_storage_based_on_address(&self, addr: &FileAddr) -> &dyn Storage {
        let storage_type = self.get_storage_type_based_on_address(addr);
        self.storage_type_to_storage(storage_type)
    }
}

impl Storage for StoragesWrapper {
    /// Stores the data into the [`default_storage_type`](`StoragesWrapper::default_storage_type`).
    fn store_stream(
        &self,
        data_stream: GenericReader,
    ) -> BoxFuture<Result<FileAddr, Box<dyn Error>>> {
        self.default_storage().store_stream(data_stream)
    }

    /// Tries to determine automatically the best [`Storage`] to retrieve data at `addr`
    /// using [`get_storage_type_based_on_address`](`StoragesWrapper::get_storage_type_based_on_address`).
    fn get_stream(&self, addr: &FileAddr) -> BoxStream<Result<Bytes, Box<dyn Error>>> {
        self.get_storage_based_on_address(addr).get_stream(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_correct_type_based_on_address(addr: FileAddr, expected_type: StorageType) {
        let wrapper = StoragesWrapper::default();
        let storage_type = wrapper.get_storage_type_based_on_address(&addr);
        assert_eq!(storage_type, expected_type);
    }

    #[test]
    fn it_chooses_the_correct_storage_type_based_on_addres() {
        let addr = "/ipfs/QmPZ9gcCEpqKTo6aq61g2nXGUhM4iCL3ewB6LDXZCtioEB".to_string();
        check_correct_type_based_on_address(addr, StorageType::Ipfs);
    }
}
