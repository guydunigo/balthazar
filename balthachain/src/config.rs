pub use web3::types::Address;

/// Configuration for the Ethereum RPC API.
#[derive(Clone, Debug)]
pub struct ChainConfig {
    /// The websocket address to connect the Ethereum json RPC endpoint.
    /// Default to `ws://localhost:8546`.
    web3_ws: String,
    /// Ethereum address to use.
    ethereum_address: Option<Address>,
    /// Password to the account.
    ethereum_password: Option<String>,
    /// Jobs contract address and path to json ABI file.
    contract_jobs: Option<(Address, Vec<u8>)>,
}

impl Default for ChainConfig {
    fn default() -> Self {
        ChainConfig {
            web3_ws: "ws://localhost:8546".to_string(),
            ethereum_address: None,
            ethereum_password: None,
            contract_jobs: None,
        }
    }
}

impl ChainConfig {
    pub fn web3_ws(&self) -> &str {
        &self.web3_ws[..]
    }
    pub fn set_web3_ws(&mut self, new: String) {
        self.web3_ws = new;
    }

    pub fn ethereum_address(&self) -> &Option<Address> {
        &self.ethereum_address
    }
    pub fn set_ethereum_address(&mut self, new: Option<Address>) {
        self.ethereum_address = new;
    }

    pub fn ethereum_password(&self) -> &Option<String> {
        &self.ethereum_password
    }
    pub fn set_ethereum_password(&mut self, new: Option<String>) {
        self.ethereum_password = new;
    }

    pub fn contract_jobs(&self) -> &Option<(Address, Vec<u8>)> {
        &self.contract_jobs
    }
    pub fn set_contract_jobs(&mut self, new: Option<(Address, Vec<u8>)>) {
        self.contract_jobs = new;
    }
}
