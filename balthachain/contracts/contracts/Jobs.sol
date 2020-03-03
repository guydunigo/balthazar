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
        bytes32[] pending_jobs;
        bytes32[] completed_jobs;
        Job[] draft_jobs;
        uint128 next_nonce;
        uint256 locked_money;
        uint256 pending_money;
    }

    uint256 public total_money;

    mapping(bytes32 => Job) jobs;
    mapping(bytes32 => Task) tasks;
    mapping(address => User) users;

    // ------------------------------------
    // User functions

    function next_nonce_and_increase() internal returns (uint128 nonce) {
        nonce = users[msg.sender].next_nonce;
        users[msg.sender].next_nonce++;
    }

    function get_pending_jobs() public view returns (bytes32[] memory) {
        return users[msg.sender].pending_jobs;
    }
    function get_completed_jobs() public view returns (bytes32[] memory) {
        return users[msg.sender].completed_jobs;
    }
    function get_draft_jobs() public view returns (uint128[] memory list) {
        list = new uint128[](users[msg.sender].draft_jobs.length);
        for (uint i = 0; i < users[msg.sender].draft_jobs.length; i++) {
            list[i] = users[msg.sender].draft_jobs[i].nonce;
        }
    }
    function get_locked_money() public view returns (uint256) {
        return users[msg.sender].locked_money;
    }
    function get_pending_money() public view returns (uint256) {
        return users[msg.sender].pending_money;
    }
    function send_pending_money() public payable {
        users[msg.sender].pending_money += msg.value;
        total_money += msg.value;
    }
    function recover_pending_money(uint256 amount) public {
        require(total_money >= amount);
        require(users[msg.sender].pending_money >= amount);

        // prevent re-entrancy attack
        // (See: https://medium.com/@gus_tavo_guim/reentrancy-attack-on-smart-contracts-how-to-identify-the-exploitable-and-an-example-of-an-attack-4470a2d8dfe4)
        users[msg.sender].pending_money -= amount;
        msg.sender.transfer(amount);
        
        // Just in case the payment doesn't go through, let's not loose count of our total money.
        total_money -= amount;
    }

    // ------------------------------------
    // Job manipulation functions

    function create_job() public {
        uint128 nonce = next_nonce_and_increase();
        users[msg.sender].draft_jobs.push(Job(
            ProgramKind.Wasm,
            new bytes[](0),
            new bytes(0),
            new bytes[](0),
            0,
            0,
            WorkerParameters (BestMethod.Cost, 0, 0, 0, 0, 0, 0),
            1,
            false,
            msg.sender,
            nonce
        ));
        emit JobNew(msg.sender, nonce);
    }

    function find_draft_program(uint128 nonce) internal view returns (uint) {
        for (uint i = 0 ; i < users[msg.sender].draft_jobs.length ; i++) {
            if (users[msg.sender].draft_jobs[i].nonce == nonce) {
                return i;
            }
        }
        require(false, "Not found.");
    }

    function get_program_kind_draft(uint128 nonce) public view returns (ProgramKind) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].program_kind;
    }
    function set_program_kind_draft(uint128 nonce, ProgramKind val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].program_kind != val, "same value");
        users[msg.sender].draft_jobs[id].program_kind = val;
    }
    function get_program_kind(bytes32 job_id) public view returns (ProgramKind) {
        return jobs[job_id].program_kind;
    }

    function get_addresses_draft(uint128 nonce) public view returns (bytes[] memory) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].addresses;
    }
    function set_addresses_draft(uint128 nonce, bytes[] memory val) public {
        uint id = find_draft_program(nonce);
        users[msg.sender].draft_jobs[id].addresses = val;
    }
    function get_addresses(bytes32 job_id) public view returns (bytes[] memory) {
        return jobs[job_id].addresses;
    }

    function get_program_hash_draft(uint128 nonce) public view returns (bytes memory) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].program_hash;
    }
    function set_program_hash_draft(uint128 nonce, bytes memory val) public {
        uint id = find_draft_program(nonce);
        users[msg.sender].draft_jobs[id].program_hash = val;
    }
    function get_program_hash(bytes32 job_id) public view returns (bytes memory) {
        return jobs[job_id].program_hash;
    }

    function get_arguments_draft(uint128 nonce) public view returns (bytes[] memory) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].arguments;
    }
    function set_arguments_draft(uint128 nonce, bytes[] memory val) public {
        uint id = find_draft_program(nonce);
        users[msg.sender].draft_jobs[id].arguments = val;
    }
    function get_arguments(bytes32 job_id) public view returns (bytes[] memory) {
        return jobs[job_id].arguments;
    }

    function get_timeout_draft(uint128 nonce) public view returns (uint64) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].timeout;
    }
    function set_timeout_draft(uint128 nonce, uint64 val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].timeout != val, "same value");
        users[msg.sender].draft_jobs[id].timeout = val;
    }
    function get_timeout(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].timeout;
    }

    function get_max_failures_draft(uint128 nonce) public view returns (uint64) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].max_failures;
    }
    function set_max_failures_draft(uint128 nonce, uint64 val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].max_failures != val, "same value");
        users[msg.sender].draft_jobs[id].max_failures = val;
    }
    function get_max_failures(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].max_failures;
    }

    function get_best_method_draft(uint128 nonce) public view returns (BestMethod) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].worker_parameters.best_method;
    }
    function set_best_method_draft(uint128 nonce, BestMethod val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].worker_parameters.best_method != val, "same value");
        users[msg.sender].draft_jobs[id].worker_parameters.best_method = val;
    }
    function get_best_method(bytes32 job_id) public view returns (BestMethod) {
        return jobs[job_id].worker_parameters.best_method;
    }

    function get_max_worker_price_draft(uint128 nonce) public view returns (uint128) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].worker_parameters.max_worker_price;
    }
    function set_max_worker_price_draft(uint128 nonce, uint128 val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].worker_parameters.max_worker_price != val, "same value");
        users[msg.sender].draft_jobs[id].worker_parameters.max_worker_price = val;
    }
    function get_max_worker_price(bytes32 job_id) public view returns (uint128) {
        return jobs[job_id].worker_parameters.max_worker_price;
    }

    function get_min_cpu_count_draft(uint128 nonce) public view returns (uint64) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].worker_parameters.min_cpu_count;
    }
    function set_min_cpu_count_draft(uint128 nonce, uint64 val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].worker_parameters.min_cpu_count != val, "same value");
        users[msg.sender].draft_jobs[id].worker_parameters.min_cpu_count = val;
    }
    function get_min_cpu_count(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].worker_parameters.min_cpu_count;
    }

    function get_min_memory_draft(uint128 nonce) public view returns (uint64) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].worker_parameters.min_memory;
    }
    function set_min_memory_draft(uint128 nonce, uint64 val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].worker_parameters.min_memory != val, "same value");
        users[msg.sender].draft_jobs[id].worker_parameters.min_memory = val;
    }
    function get_min_memory(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].worker_parameters.min_memory;
    }

    function get_max_network_usage_draft(uint128 nonce) public view returns (uint64) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].worker_parameters.max_network_usage;
    }
    function set_max_network_usage_draft(uint128 nonce, uint64 val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].worker_parameters.max_network_usage != val, "same value");
        users[msg.sender].draft_jobs[id].worker_parameters.max_network_usage = val;
    }
    function get_max_network_usage(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].worker_parameters.max_network_usage;
    }

    function get_max_network_price_draft(uint128 nonce) public view returns (uint128) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].worker_parameters.max_network_price;
    }
    function set_max_network_price_draft(uint128 nonce, uint128 val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].worker_parameters.max_network_price != val, "same value");
        users[msg.sender].draft_jobs[id].worker_parameters.max_network_price = val;
    }
    function get_max_network_price(bytes32 job_id) public view returns (uint128) {
        return jobs[job_id].worker_parameters.max_network_price;
    }

    function get_min_network_speed_draft(uint128 nonce) public view returns (uint64) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].worker_parameters.min_network_speed;
    }
    function set_min_network_speed_draft(uint128 nonce, uint64 val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].worker_parameters.min_network_speed != val, "same value");
        users[msg.sender].draft_jobs[id].worker_parameters.min_network_speed = val;
    }
    function get_min_network_speed(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].worker_parameters.min_network_speed;
    }

    function get_redundancy_draft(uint128 nonce) public view returns (uint64) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].redundancy;
    }
    function set_redundancy_draft(uint128 nonce, uint64 val) public {
        require(val >= 1, "redundancy should be 1 on more");
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].redundancy != val, "same value");
        users[msg.sender].draft_jobs[id].redundancy = val;
    }
    function get_redundancy(bytes32 job_id) public view returns (uint64) {
        return jobs[job_id].redundancy;
    }

    function get_is_program_pure_draft(uint128 nonce) public view returns (bool) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].is_program_pure;
    }
    function set_is_program_pure_draft(uint128 nonce, bool val) public {
        uint id = find_draft_program(nonce);
        require(users[msg.sender].draft_jobs[id].is_program_pure != val, "same value");
        users[msg.sender].draft_jobs[id].is_program_pure = val;
    }
    function get_is_program_pure(bytes32 job_id) public view returns (bool) {
        return jobs[job_id].is_program_pure;
    }

    function get_sender(bytes32 job_id) public view returns (address) {
        return jobs[job_id].sender;
    }

    function get_nonce_draft(uint128 nonce) public view returns (uint128) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].nonce;
    }
    function get_nonce(bytes32 job_id) public view returns (uint128) {
        return jobs[job_id].nonce;
    }

    function is_draft_ready(Job storage job) private view returns (bool) {
        return job.addresses.length > 0
            && job.program_hash.length > 0
            && job.arguments.length > 0

            && job.timeout > 0
            && job.max_failures > 0

            && job.worker_parameters.max_worker_price > 0

            && users[msg.sender].pending_money >= calculate_max_price(job);
    }
    function is_draft_ready_nonce(uint128 nonce) public view returns (bool) {
        Job storage job = users[msg.sender].draft_jobs[find_draft_program(nonce)];
        return is_draft_ready(job);
    }

    function delete_draft(uint id) public {
        uint len = users[msg.sender].draft_jobs.length;
        User storage user = users[msg.sender];

        user.draft_jobs[id] = user.draft_jobs[len - 1];
        delete user.draft_jobs[len-1];
        user.draft_jobs.length--;
    }

    function calculate_max_price(Job storage job) internal view returns (uint) {
        return (job.timeout * job.worker_parameters.max_worker_price
        + job.worker_parameters.max_network_usage * job.worker_parameters.max_network_price)
        * job.redundancy * job.arguments.length;
    }

    function calculate_max_price_nonce(uint128 nonce) public view returns (uint) {
        return calculate_max_price(users[msg.sender].draft_jobs[find_draft_program(nonce)]);
    }

    function ready(uint128 nonce) public {
        uint id = find_draft_program(nonce);
        Job storage job = users[msg.sender].draft_jobs[id];
        require(is_draft_ready(job), "Draft doesn't meet all the conditions to be sent.");

        /*
        delete_draft(id);
        emit JobPending(job_id);
        */
    }

    // ------------------------------------------------------------
    event JobNew(address sender, uint128 nonce);
    event CounterHasNewValue(uint128 new_counter);
    event JobPending(bytes32 job_id);

    event TaskNewResult(uint64 job_id, uint64 task_id, bytes result);
}
