pragma solidity >=0.4.21 <0.7.0;
// pragma experimental ABIEncoderV2;

contract Jobs {
    enum ProgramKind { Wasm }
    enum BestMethod { Performance, Cost }

    struct Arguments {
        bytes arguments;
        bytes[] result;
    }

    struct Job {
        ProgramKind program_kind;
        bytes[] addresses;
        bytes program_hash;
        uint64 next_arguments_id;
        mapping(uint => Arguments) arguments;

        uint64 timeout;
        uint64 max_failures;

        BestMethod best_method;
        WorkerParameters worker_parameters;

        uint64 redundancy;
        bool includes_tests;

        address sender;

        bool is_locked;
    }

    struct WorkerParameters {
        uint64 max_worker_price;
        uint64 min_cpu_count;
        uint64 min_memory;
        uint64 min_network_speed;
        uint64 max_network_usage;
    }

    Job[] jobs;

    function send_wasm_job(
        bytes memory program_hash,
        uint64 timeout,
        uint64 max_failures,
        BestMethod best_method,
        uint64 max_worker_price,
        uint64 min_cpu_count,
        uint64 min_memory,
        uint64 min_network_speed,
        uint64 max_network_usage,
        uint64 redundancy
    ) public returns (uint64 job_id) {
        jobs.push(Job(ProgramKind.Wasm,
                      new bytes[](1),
                      program_hash,
                      0,
                      timeout,
                      max_failures,
                      best_method,
                      WorkerParameters (
                          max_worker_price,
                          min_cpu_count,
                          min_memory,
                          min_network_speed,
                          max_network_usage
                      ),
                      redundancy,
                      true,
                      msg.sender,
                      false
                     ));

        return uint64(jobs.length - 1);
    }

    function push_program_address(uint64 job_id, bytes memory program_address) public {
        require(job_id < jobs.length, "Unknown job id");
        require(jobs[job_id].is_locked == false, "Job is marked as locked.");
        require(jobs[job_id].sender == msg.sender, "Not authorized: not job sender");

        jobs[job_id].addresses.push(program_address);
    }

    function push_program_arguments(uint64 job_id, bytes memory arguments) public returns (uint64 arg_id) {
        require(job_id < jobs.length, "Unknown job id");
        require(jobs[job_id].is_locked == false, "Job is marked as locked.");
        require(jobs[job_id].sender == msg.sender, "Not authorized: not job sender");

        uint64 r = jobs[job_id].redundancy;
        arg_id = jobs[job_id].next_arguments_id;
        jobs[job_id].next_arguments_id++;

        jobs[job_id].arguments[arg_id] = Arguments(arguments, new bytes[](r));
    }

    function lock(uint64 job_id) public {
        require(job_id < jobs.length, "Unknown job id");
        require(jobs[job_id].sender == msg.sender, "Not authorized: not job sender");

        jobs[job_id].is_locked = true;
        emit JobNew(job_id);
    }

    function get_job(uint64 job_id) public view returns (
        ProgramKind,
        bytes memory,
        uint64,
        uint64,
        BestMethod,
        uint64,
        uint64,
        uint64,
        uint64,
        uint64,
        uint64,
        bool includes_tests
    ) {
        require(job_id < jobs.length, "unknown job id");

        Job memory job = jobs[job_id];
        return (
            job.program_kind,
            job.program_hash,
            job.timeout,
            job.max_failures,
            job.best_method,
            job.worker_parameters.max_worker_price,
            job.worker_parameters.min_cpu_count,
            job.worker_parameters.min_memory,
            job.worker_parameters.min_network_speed,
            job.worker_parameters.max_network_usage,
            job.redundancy,
            job.includes_tests
        );
    }

    event JobNew(uint64 job_id);
    event TaskDone(uint64 job_id, uint64 task_id);
}
