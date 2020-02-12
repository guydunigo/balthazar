use super::ipfs;
use multiaddr::Multiaddr;

/// This enum defines the different [`StorageType`] available for
/// [`StoragesWrapper`](`super::StoragesWrapper`).
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

/// Configuration used by [`StoragesWrapper`](`super::StoragesWrapper`)
#[derive(Clone, Default, Debug)]
pub struct StorageConfig {
    /// The address to connect the IPFS API, see [the default implementation of
    /// IpfsClient](`ipfs_api::IpfsClient`).
    ipfs_api: Option<Multiaddr>,
    /// Default storage type see [`StoragesWrapper`](`super::StoragesWrapper`) for more
    /// information.
    default_storage: StorageType,
}

impl StorageConfig {
    pub fn ipfs_api(&self) -> &Option<Multiaddr> {
        &self.ipfs_api
    }
    /// Returns an Error if the address provided can't be used to access the IPFS API.
    pub fn set_ipfs_api(
        &mut self,
        new: Option<Multiaddr>,
    ) -> Result<(), ipfs::IpfsStorageCreationError> {
        if let Some(ref addr) = new {
            ipfs::IpfsStorage::new(addr.clone())?;
        }
        self.ipfs_api = new;
        Ok(())
    }

    pub fn default_storage(&self) -> &StorageType {
        &self.default_storage
    }
    pub fn set_default_storage(&mut self, new: StorageType) {
        self.default_storage = new;
    }
}
