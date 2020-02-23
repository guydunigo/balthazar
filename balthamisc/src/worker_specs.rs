/// Technical specifications of a worker useful to estimate its performance
/// and adequacy to tasks.
#[derive(Clone, Copy, Debug)]
pub struct WorkerSpecs {
    /// Indicates how many parallel tasks can be executed at the same time.
    cpu_count: u64,
    // TODO: use file size library for correct unit handling
    /// Maximum Random Access Memory amount that can be used by tasks in kilobytes.
    memory: u64,
    /// Maximum network speed in kilobits per seconds that tasks can use in total.
    /// Set to `0` to disable networking completely.
    network_speed: u64,
    /// Price of our worker in money / second.
    worker_price: u64,
}

impl Default for WorkerSpecs {
    fn default() -> Self {
        WorkerSpecs {
            // TODO: auto detect
            cpu_count: 4,
            // TODO: auto detect / dynamic ?
            // TODO: use file size library for correct unit handling
            memory: 1024,
            // TODO: auto detect
            network_speed: 10,
            // TODO: based on market average  ?
            worker_price: 10,
        }
    }
}
