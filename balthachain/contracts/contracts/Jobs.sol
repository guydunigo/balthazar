pragma solidity >=0.4.21 <0.7.0;
pragma experimental ABIEncoderV2;

contract Jobs {
    uint128 public counter;

    function set_counter(uint128 new_val) public {
        require(counter != new_val);
        counter = new_val;
        emit CounterHasNewValue(counter);
    }

    function inc_counter() public {
        counter++;
        emit CounterHasNewValue(counter);
    }

    enum ProgramKind { Wasm }
    enum BestMethod { Performance, Cost }

    struct Task {
        bytes arguments;
        bytes[] result;
    }

    struct Job {
        ProgramKind program_kind;
        bytes[] addresses;
        bytes program_hash;
        uint64 next_task_id;
        mapping(uint => Task) tasks;

        uint64 timeout;
        uint64 max_failures;

        WorkerParameters worker_parameters;

        uint64 redundancy;
        bool includes_tests;

        address sender;

        bool is_locked;
    }

    struct WorkerParameters {
        BestMethod best_method;
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
    ) public {
        jobs.push(Job(ProgramKind.Wasm,
                      new bytes[](0),
                      program_hash,
                      0,
                      timeout,
                      max_failures,
                      WorkerParameters (
                          best_method,
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

        emit JobNew(msg.sender, uint64(jobs.length - 1));
    }

    function set_includes_tests(uint64 job_id, bool val) public {
        require(job_id < jobs.length, "Unknown job id");
        require(jobs[job_id].is_locked == false, "Job is marked as locked.");
        require(jobs[job_id].sender == msg.sender, "Not authorized: not job sender.");
        require(jobs[job_id].includes_tests != val, "Already at that value.");

        jobs[job_id].includes_tests = val;
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
        arg_id = jobs[job_id].next_task_id;
        jobs[job_id].next_task_id++;

        jobs[job_id].tasks[arg_id] = Task(arguments, new bytes[](r));
    }

    function lock(uint64 job_id) public {
        require(job_id < jobs.length, "Unknown job id");
        require(jobs[job_id].is_locked == false, "Job is already marked as locked.");
        require(jobs[job_id].sender == msg.sender, "Not authorized: not job sender");

        jobs[job_id].is_locked = true;
        emit JobLocked(job_id);
    }

    function get_jobs_length() public view returns (uint128) {
        return uint128(jobs.length);
    }

    function get_job(uint64 job_id) public view returns (
        ProgramKind,
        bytes memory,
        uint64,
        uint64,
        uint64,
        bool,
        address
    ) {
        require(job_id < jobs.length, "Unknown job id");

        Job memory job = jobs[job_id];
        return (
            job.program_kind,
            job.program_hash,
            job.timeout,
            job.max_failures,
            job.redundancy,
            job.includes_tests,
            job.sender
        );
    }

    function get_worker_parameters(uint64 job_id) public view returns (
        BestMethod,
        uint64,
        uint64,
        uint64,
        uint64,
        uint64
    ) {
        require(job_id < jobs.length, "Unknown job id");

        Job memory job = jobs[job_id];
        return (
            job.worker_parameters.best_method,
            job.worker_parameters.max_worker_price,
            job.worker_parameters.min_cpu_count,
            job.worker_parameters.min_memory,
            job.worker_parameters.min_network_speed,
            job.worker_parameters.max_network_usage
        );
    }

    function get_addresses(uint64 job_id) public view returns (bytes[] memory) {
        require(job_id < jobs.length, "Unknown job id");
        return jobs[job_id].addresses;
    }

    function get_arguments_length(uint64 job_id) public view returns (uint64) {
        require(job_id < jobs.length, "Unknown job id");
        return jobs[job_id].next_task_id;
    }

    function get_arguments(uint64 job_id, uint64 task_id) public view returns (bytes memory) {
        require(job_id < jobs.length, "Unknown job id");
        require(task_id < get_arguments_length(job_id), "Unknown task id for this job");
        return jobs[job_id].tasks[task_id].arguments;
    }

    function set_result(uint64 job_id, uint64 task_id, bytes memory result) public {
        require(job_id < jobs.length, "Unknown job id");
        require(task_id < get_arguments_length(job_id), "Unknown task id for this job");
        require(jobs[job_id].tasks[task_id].result.length == 0, "Already has a result");

        jobs[job_id].tasks[task_id].result.push(result);
        emit TaskNewResult(job_id, task_id, result);
    }

    function get_result(uint64 job_id, uint64 task_id) public view returns (bytes memory) {
        require(job_id < jobs.length, "Unknown job id");
        require(task_id < get_arguments_length(job_id), "Unknown task id for this job");
        require(jobs[job_id].tasks[task_id].result.length > 0, "No result");
        return jobs[job_id].tasks[task_id].result[0];
    }

    event JobNew(address sender, uint64 job_id);
    event JobLocked(uint64 job_id);
    event TaskNewResult(uint64 job_id, uint64 task_id, bytes result);
    event CounterHasNewValue(uint128 new_counter);
}
