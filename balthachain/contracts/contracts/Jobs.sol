pragma solidity >=0.4.21 <0.7.0;
pragma experimental ABIEncoderV2;

contract Jobs {
    uint128 public counter;

    function set_counter(uint128 new_val) public {
        require(counter != new_val, "Same value as before.");
        counter = new_val;
        emit CounterHasNewValue(counter);
    }

    function inc_counter() public {
        counter++;
        emit CounterHasNewValue(counter);
    }

    enum ProgramKind { Wasm }
    enum BestMethod { Cost, Performance }

    struct Job {
        ProgramKind program_kind;
        bytes[] addresses;
        bytes program_hash;
        bytes[] arguments;

        uint64 timeout;
        uint64 max_failures;

        WorkerParameters worker_parameters;

        uint64 redundancy;
        bool is_program_pure;

        address sender;
        uint128 nonce;
    }

    struct WorkerParameters {
        BestMethod best_method;
        uint128 max_worker_price;
        uint64 min_cpu_count;
        uint64 min_memory;
        uint64 max_network_usage;
        uint128 max_network_price;
        uint64 min_network_speed;
    }

    struct Task {
        bytes32 job_id;
        bytes arguments;
        bytes[] result;
    }

    struct User {
        bytes32[] jobs;
        Job[] pending_jobs;
        uint128 next_nonce;
        uint256 money;
    }

    mapping(bytes32 => Job) jobs;
    mapping(bytes32 => Task) tasks;
    mapping(address => User) users;

    function next_nonce_and_increase() internal returns (uint128 nonce) {
        nonce = users[msg.sender].next_nonce;
        users[msg.sender].next_nonce++;
    }

    function create_job() public {
        uint128 nonce = next_nonce_and_increase();
        users[msg.sender].pending_jobs.push(Job(
            ProgramKind.Wasm,
            new bytes[](0),
            new bytes(0),
            new bytes[](0),
            0,
            0,
            WorkerParameters (BestMethod.Cost, 0, 0, 0, 0, 0, 0),
            0,
            false,
            msg.sender,
            nonce
        ));
        emit JobNew(msg.sender, nonce);
    }

    function find_pending_program(uint64 nonce) internal view returns (uint) {
        for (uint i = 0 ; i < users[msg.sender].pending_jobs.length ; i++) {
            if (users[msg.sender].pending_jobs[i].nonce == nonce) {
                return i;
            }
        }
        require(false, "Not found.");
    }

    function get_program_kind_pending(uint64 nonce) public view returns (ProgramKind) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].program_kind;
    }
    function set_program_kind_pending(uint64 nonce, ProgramKind val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].program_kind = val;
    }
    function get_program_kind(bytes32 job_id) public view returns (ProgramKind) {
        return jobs[job_id].program_kind;
    }
    function set_program_kind_pending(bytes32 job_id, ProgramKind val) public {
        jobs[job_id].program_kind = val;
    }

    function get_addresses_pending(uint64 nonce) public view returns (bytes[] memory) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].addresses;
    }
    function set_addresses_pending(uint64 nonce, bytes[] memory val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].addresses = val;
    }
    function get_addresses(bytes32 job_id) public view returns (bytes[] memory) {
        return jobs[job_id].addresses;
    }
    function set_addresses(bytes32 job_id, bytes[] memory val) public {
        jobs[job_id].addresses = val;
    }

    function get_program_hash_pending(uint64 nonce) public view returns (bytes memory) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].program_hash;
    }
    function set_program_hash_pending(uint64 nonce, bytes memory val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].program_hash = val;
    }
    function get_program_hash(bytes32 job_id) public view returns (bytes memory) {
        return jobs[job_id].program_hash;
    }
    function set_program_hash(bytes32 job_id, bytes memory val) public {
        jobs[job_id].program_hash = val;
    }

    function get_arguments_pending(uint64 nonce) public view returns (bytes[] memory) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].arguments;
    }
    function set_arguments_pending(uint64 nonce, bytes[] memory val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].arguments = val;
    }
    function get_arguments(bytes32 job_id) public view returns (bytes[] memory) {
        return jobs[job_id].arguments;
    }
    function set_arguments(bytes32 job_id, bytes[] memory val) public {
        jobs[job_id].arguments = val;
    }

    function get_timeout_pending(uint64 nonce) public view returns (uint64) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].timeout;
    }
    function set_timeout_pending(uint64 nonce, uint64 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].timeout = val;
    }
    function get_timeout(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].timeout;
    }
    function set_timeout(bytes32 job_id, uint64 val) public {
        jobs[job_id].timeout = val;
    }

    function get_max_failures_pending(uint64 nonce) public view returns (uint64) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].max_failures;
    }
    function set_max_failures_pending(uint64 nonce, uint64 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].max_failures = val;
    }
    function get_max_failures(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].max_failures;
    }
    function set_max_failures(bytes32 job_id, uint64 val) public {
        jobs[job_id].max_failures = val;
    }

    function get_best_method_pending(uint64 nonce) public view returns (BestMethod) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.best_method;
    }
    function set_best_method_pending(uint64 nonce, BestMethod val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.best_method = val;
    }
    function get_best_method(bytes32 job_id) public view returns (BestMethod) {
        return jobs[job_id].worker_parameters.best_method;
    }
    function set_best_method(bytes32 job_id, BestMethod val) public {
        jobs[job_id].worker_parameters.best_method = val;
    }

    function get_max_worker_price_pending(uint64 nonce) public view returns (uint128) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.max_worker_price;
    }
    function set_max_worker_price_pending(uint64 nonce, uint128 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.max_worker_price = val;
    }
    function get_max_worker_price(bytes32 job_id) public view returns (uint128) {
        return jobs[job_id].worker_parameters.max_worker_price;
    }
    function set_max_worker_price(bytes32 job_id, uint128 val) public {
        jobs[job_id].worker_parameters.max_worker_price = val;
    }

    function get_min_cpu_count_pending(uint64 nonce) public view returns (uint64) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.min_cpu_count;
    }
    function set_min_cpu_count_pending(uint64 nonce, uint64 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.min_cpu_count = val;
    }
    function get_min_cpu_count(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].worker_parameters.min_cpu_count;
    }
    function set_min_cpu_count(bytes32 job_id, uint64 val) public {
        jobs[job_id].worker_parameters.min_cpu_count = val;
    }

    function get_min_memory_pending(uint64 nonce) public view returns (uint64) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.min_memory;
    }
    function set_min_memory_pending(uint64 nonce, uint64 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.min_memory = val;
    }
    function get_min_memory(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].worker_parameters.min_memory;
    }
    function set_min_memory(bytes32 job_id, uint64 val) public {
        jobs[job_id].worker_parameters.min_memory = val;
    }

    function get_max_network_usage_pending(uint64 nonce) public view returns (uint64) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.max_network_usage;
    }
    function set_max_network_usage_pending(uint64 nonce, uint64 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.max_network_usage = val;
    }
    function get_max_network_usage(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].worker_parameters.max_network_usage;
    }
    function set_max_network_usage(bytes32 job_id, uint64 val) public {
        jobs[job_id].worker_parameters.max_network_usage = val;
    }

    function get_max_network_price_pending(uint64 nonce) public view returns (uint128) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.max_network_price;
    }
    function set_max_network_price_pending(uint64 nonce, uint128 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.max_network_price = val;
    }
    function get_max_network_price(bytes32 job_id) public view returns (uint128) {
        return jobs[job_id].worker_parameters.max_network_price;
    }
    function set_max_network_price(bytes32 job_id, uint128 val) public {
        jobs[job_id].worker_parameters.max_network_price = val;
    }

    function get_min_network_speed_pending(uint64 nonce) public view returns (uint64) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.min_network_speed;
    }
    function set_min_network_speed_pending(uint64 nonce, uint64 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].worker_parameters.min_network_speed = val;
    }
    function get_min_network_speed(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].worker_parameters.min_network_speed;
    }
    function set_min_network_speed(bytes32 job_id, uint64 val) public {
        jobs[job_id].worker_parameters.min_network_speed = val;
    }

    function get_redundancy_pending(uint64 nonce) public view returns (uint64) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].redundancy;
    }
    function set_redundancy_pending(uint64 nonce, uint64 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].redundancy = val;
    }
    function get_redundancy(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].redundancy;
    }
    function set_redundancy(bytes32 job_id, uint64 val) public {
        jobs[job_id].redundancy = val;
    }

    function get_is_program_pure_pending(uint64 nonce) public view returns (bool) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].is_program_pure;
    }
    function set_is_program_pure_pending(uint64 nonce, bool val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].is_program_pure = val;
    }
    function get_is_program_pure(bytes32 job_id) public view returns (bool) {
        return jobs[job_id].is_program_pure;
    }
    function set_is_program_pure(bytes32 job_id, bool val) public {
        jobs[job_id].is_program_pure = val;
    }

    function get_nonce_pending(uint64 nonce) public view returns (uint128) {
        return users[msg.sender].pending_jobs[find_pending_program(nonce)].nonce;
    }
    function set_nonce_pending(uint64 nonce, uint128 val) public {
        users[msg.sender].pending_jobs[find_pending_program(nonce)].nonce = val;
    }
    function get_nonce(bytes32 job_id) public view returns (uint128) {
        return jobs[job_id].nonce;
    }
    function set_nonce(bytes32 job_id, uint128 val) public {
        jobs[job_id].nonce = val;
    }

    // ------------------------------------------------------------

    /*
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

        jobs[job_id].tasks[arg_id] = Task(arguments, new bytes[](0));
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

    event JobLocked(uint64 job_id);
    event TaskNewResult(uint64 job_id, uint64 task_id, bytes result);
    */
    event JobNew(address sender, uint128 nonce);
    event CounterHasNewValue(uint128 new_counter);
}
