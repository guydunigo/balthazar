use proto::worker::ProgramKind;

/// Technical specifications of a worker useful to estimate its performance
/// and adequacy to tasks.
#[derive(Clone, Debug)]
pub struct WorkerSpecs {
    /// Price of our worker in money / second.
    worker_price: u64,
    /// Network price in money / kilobits
    network_price: u64,
    /// Indicates how many parallel tasks can be executed at the same time.
    cpu_count: u64,
    // TODO: use file size library for correct unit handling
    /// Maximum Random Access Memory amount that can be used by tasks in kilobytes.
    memory: u64,
    /// Maximum network speed in kilobits per seconds that tasks can use in total.
    /// Set to `0` to disable networking completely.
    network_speed: u64,
    /// Supported program kinds.
    supported_program_kinds: Vec<ProgramKind>,
}

impl WorkerSpecs {
    pub fn new(
        worker_price: u64,
        network_price: u64,
        cpu_count: u64,
        memory: u64,
        network_speed: u64,
        supported_program_kinds: Vec<ProgramKind>,
    ) -> Self {
        WorkerSpecs {
            worker_price,
            network_price,
            cpu_count,
            memory,
            network_speed,
            supported_program_kinds,
        }
    }

    pub fn worker_price(&self) -> u64 {
        self.worker_price
    }

    pub fn set_worker_price(&mut self, new: u64) {
        self.worker_price = new;
    }

    pub fn network_price(&self) -> u64 {
        self.network_price
    }

    pub fn set_network_price(&mut self, new: u64) {
        self.network_price = new;
    }

    pub fn cpu_count(&self) -> u64 {
        self.cpu_count
    }

    pub fn set_cpu_count(&mut self, new: u64) {
        self.cpu_count = new;
    }

    pub fn memory(&self) -> u64 {
        self.memory
    }

    pub fn set_memory(&mut self, new: u64) {
        self.memory = new;
    }

    pub fn network_speed(&self) -> u64 {
        self.network_speed
    }

    pub fn set_network_speed(&mut self, new: u64) {
        self.network_speed = new;
    }

    pub fn supported_program_kinds(&self) -> &[ProgramKind] {
        &self.supported_program_kinds[..]
    }

    pub fn supported_program_kinds_mut(&mut self) -> &mut Vec<ProgramKind> {
        &mut self.supported_program_kinds
    }

    pub fn supported_program_kinds_proto(&self) -> Vec<i32> {
        self.supported_program_kinds
            .iter()
            .map(|i| (*i).into())
            .collect()
    }
}

impl Default for WorkerSpecs {
    fn default() -> Self {
        WorkerSpecs {
            // TODO: based on market average  ?
            worker_price: 10,
            // TODO: based on market average  ?
            network_price: 10,
            // TODO: auto detect
            cpu_count: 4,
            // TODO: auto detect / dynamic ?
            // TODO: use file size library for correct unit handling
            memory: 1024,
            // TODO: auto detect
            network_speed: 10,
            // TODO: auto detect based on availability of Docker...
            supported_program_kinds: vec![ProgramKind::Wasm0m1n0],
        }
    }
}
