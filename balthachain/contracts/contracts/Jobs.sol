pragma solidity >=0.4.21 <0.7.0;
pragma experimental ABIEncoderV2;

contract Jobs {
    /*
    uint128 public counter;

    function set_counter(uint128 new_val) public {
        // require(counter != new_val, "Same value as before.");
        counter = new_val;
        emit CounterHasNewValue(counter);
    }

    function inc_counter() public {
        counter++;
        emit CounterHasNewValue(counter);
    }
    event CounterHasNewValue(uint128 new_counter);
    */

    enum TaskState {
        Incomplete,
        Completed,
        DefinetelyFailed
    }

    enum TaskDefiniteErrorKind {
        TimedOut,
        Download,
        Runtime,
        IncorrectSpecification,
        IncorrectResult
    }

    struct Job {
        bytes[] arguments;
        uint64 timeout;
        uint64 max_worker_price;
        uint64 max_network_usage;
        uint64 max_network_price;

        uint64 min_checking_interval;
        uint64 management_price;

        uint64 redundancy;
        uint64 max_failures;

        bytes other_data;

        address sender;
        uint128 nonce;

        bool is_draft;
        bool non_null;
    }

    struct Task {
        bytes32 job_id;
        uint128 argument_id;
        TaskState state;
        address[] managers_addresses;

        bytes result;
        /*
        address[] workers_addresses;
        uint64[] worker_prices;
        uint64[] network_prices;
        */

        TaskDefiniteErrorKind reason;

        bool non_null;
    }

    struct User {
        // don't need to store that, just go through all nonces so all jobs for this user...
        // bytes32[] pending_jobs;
        // bytes32[] completed_jobs;
        // Job[] draft_jobs;
        uint128 next_nonce;
        uint256 locked_money;
        uint256 pending_money;
    }

    mapping(bytes32 => Job) jobs;
    mapping(bytes32 => Task) tasks;
    mapping(address => User) users;

    // TODO: ability to set this address ?
    address public oracle;

    constructor(/*address oracle_address*/) public {
        // oracle = oracle_address;
        oracle = msg.sender;
    }

    // ------------------------------------
    // Job and tasks tool functions

    function calc_max_price_per_task(Job storage job) internal view returns (uint) {
        return (job.timeout * job.max_worker_price
        + job.max_network_usage * job.max_network_price)
        * job.redundancy
        + job.management_price
        * (job.redundancy + job.max_failures)
        * (4 + job.timeout / job.min_checking_interval);
    }

    function calc_max_price(Job storage job) internal view returns (uint) {
        return calc_max_price_per_task(job) * job.arguments.length;
    }

    function calc_job_id(address sender, uint128 nonce) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(sender, nonce));
    }

    function calc_task_id(bytes32 job_id, uint128 index/*, bytes storage argument*/) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(job_id, index/*, argument*/));
    }

    function is_draft_ready(Job storage job) internal view returns (bool) {
        return job.arguments.length > 0 &&

            job.timeout > 9 &&

            job.max_worker_price > 0 &&
            job.max_network_price > 0 &&
            job.redundancy > 0 &&
            job.min_checking_interval > 14 &&
            job.management_price > 0 &&

            users[msg.sender].pending_money >= calc_max_price(job);
    }

    /*
    // TODO: useful? or directly store drafts to usual jobs...
    function find_draft_program(uint128 nonce) internal view returns (uint) {
        for (uint i ; i < users[msg.sender].is_draft_jobs.length ; i++) {
            if (users[msg.sender].is_draft_jobs[i].nonce == nonce) {
                return i;
            }
        }
        revert("Not found.");
    }
    */

    function pay_managers(Task storage task) internal returns (uint256){
        require(task.managers_addresses.length > 0/*, no managers set*/);
        // TODO: check max size
        Job storage job = jobs[task.job_id];

        for (uint64 i ; i < task.managers_addresses.length; i++) {
            users[task.managers_addresses[i]].pending_money += job.management_price;
            emit PendingMoneyChanged(task.managers_addresses[i], users[task.managers_addresses[i]].pending_money);
        }

        return job.management_price * task.managers_addresses.length;
    }

    // Reverts if there is no job corresponding to `job_id`.
    function is_job_completed(bytes32 job_id) public view returns (bool result) {
        require(jobs[job_id].non_null/*, "unknown job"*/);

        result = true;
        for (uint128 i ; i < jobs[job_id].arguments.length ; i++) {
            if (tasks[calc_task_id(job_id, i/*, jobs[job_id].arguments[i]*/)].state == TaskState.Incomplete) {
                result = false;
                break;
            }
        }
    }

    // ------------------------------------
    // User functions

    // Defines the next nonce that will be used for a draft meaning there may exist a job
    // (or at least a draft) for each : `0 <= nonce < next_nonce`.
    // TODO: restrict to sender ?
    function get_next_nonce() public view returns (uint128) {
        return users[msg.sender].next_nonce;
    }

    function next_nonce_and_increase() internal returns (uint128 nonce) {
        nonce = users[msg.sender].next_nonce;
        users[msg.sender].next_nonce++;
    }

    /*
    function get_pending_jobs() public view returns (bytes32[] memory) {
        return users[msg.sender].pending_jobs;
    }
    */

    /*
    function get_completed_jobs() public view returns (bytes32[] memory) {
        return users[msg.sender].completed_jobs;
    }
    */

    /*
    function get_draft_jobs() public view returns (uint128[] memory list) {
        Job[] storage draft_jobs = users[msg.sender].is_draft_jobs;
        list = new uint128[](draft_jobs.length);
        for (uint i; i < draft_jobs.length; i++) {
            list[i] = draft_jobs[i].nonce;
        }
    }
    */
    // TODO: restrict to sender ?
    function get_pending_locked_money() public view returns (uint256, uint256) {
        return (users[msg.sender].pending_money, users[msg.sender].locked_money);
    }
    // TODO: restrict to sender ?
    function send_pending_money() public payable {
        users[msg.sender].pending_money += msg.value;
        emit PendingMoneyChanged(msg.sender, users[msg.sender].pending_money);
    }
    // Reverts if there is not enough money in user's pending money.
    function recover_pending_money(uint256 amount) public {
        require(users[msg.sender].pending_money >= amount/*, "too few in pending"*/);

        // prevent re-entrancy attack
        // (See: https://medium.com/@gus_tavo_guim/reentrancy-attack-on-smart-contracts-how-to-identify-the-exploitable-and-an-example-of-an-attack-4470a2d8dfe4)
        users[msg.sender].pending_money -= amount;
        msg.sender.transfer(amount);
        emit PendingMoneyChanged(msg.sender, users[msg.sender].pending_money);
    }

    // ------------------------------------
    // Job manipulation functions

    function create_draft() public {
        uint128 nonce;
        do {
            nonce = next_nonce_and_increase();
        } while (jobs[calc_job_id(msg.sender, nonce)].non_null);

        jobs[calc_job_id(msg.sender, nonce)] = Job(
            new bytes[](0),
            10,
            1,
            0,
            1,
            15,
            1,
            1,
            0,
            new bytes(0),
            msg.sender,
            nonce,
            true,
            true
        );
        emit JobNew(msg.sender, nonce);
    }
    function check_is_our_draft(Job storage job) internal view {
        require(job.non_null/*, "null job"*/);
        require(job.sender == msg.sender/*, "job's sender isn't the message's sender"*/);
        require(job.is_draft/*, "job already locked ready"*/);
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_parameters(bytes32 job_id) public view returns (
        uint64,
        uint64,
        uint64
    ) {
        Job storage job = jobs[job_id];
        require(job.non_null/*, "unknown job"*/);
        return (
            job.timeout,
            job.redundancy,
            job.max_failures
        );
    }
    // Reverts if `timeout` or `max_failures` is null.
    function set_parameters(
        bytes32 job_id,
        uint64 timeout,
        uint64 redundancy,
        uint64 max_failures//,
    ) public {
        require(timeout > 9 && redundancy > 0/*, "invalid data"*/);
        Job storage job = jobs[job_id];
        check_is_our_draft(job);

        job.timeout = timeout;
        job.redundancy = redundancy;
        job.max_failures = max_failures;
    }

    // Reverts if there is no job corresponding to `job_id`.
    function get_other_data(bytes32 job_id) public view returns (bytes memory) {
        Job storage job = jobs[job_id];
        require(job.non_null/*, "unknown job"*/);
        return job.other_data;
    }
    function set_other_data(bytes32 job_id, bytes memory val) public {
        Job storage job = jobs[job_id];
        check_is_our_draft(job);

        job.other_data = val;
    }

    // Reverts if there is no job corresponding to `job_id`.
    function get_arguments(bytes32 job_id) public view returns (bytes[] memory) {
        Job storage job = jobs[job_id];
        require(job.non_null/*, "unknown job"*/);
        return job.arguments;
    }
    // Reverts if the provided array is empty.
    function reset_arguments(bytes32 job_id) public {
        Job storage job = jobs[job_id];
        check_is_our_draft(job);
        delete job.arguments;
        job.arguments = new bytes[](0);
    }
    function push_argument(bytes32 job_id, bytes memory val) public {
        require(val.length > 0/*, "empty array"*/);
        Job storage job = jobs[job_id];
        check_is_our_draft(job);

        job.arguments.push(val);
    }

    // Reverts if there is no job corresponding to `job_id`.
    function get_worker_parameters(bytes32 job_id) public view returns (
        uint64,
        uint64,
        uint64
    ) {
        Job storage job = jobs[job_id];
        require(job.non_null/*, "unknown job"*/);

        return (
            job.max_worker_price,
            job.max_network_usage,
            job.max_network_price// ,
        );
    }
    // Reverts if `max_worker_price` or `max_network_price` is null.
    function set_worker_parameters(
        bytes32 job_id,
        uint64 max_worker_price,
        uint64 max_network_usage,
        uint64 max_network_price
    ) public {
        require(max_worker_price > 0 && max_network_price > 0/*, "invalid data"*/);
        Job storage job = jobs[job_id];
        check_is_our_draft(job);

        job.max_worker_price = max_worker_price;
        job.max_network_usage = max_network_usage;
        job.max_network_price = max_network_price;
    }

    // Reverts if there is no job corresponding to `job_id`.
    function get_management_parameters(bytes32 job_id) public view returns (
        uint64,
        uint64
    ) {
        Job storage job = jobs[job_id];
        require(job.non_null/*, "unknown job"*/);

        return (
            job.min_checking_interval,
            job.management_price
        );
    }
    // Reverts if `min_checking_interval` or `management_price` is null.
    function set_management_parameters(
        bytes32 job_id,
        uint64 min_checking_interval,
        uint64 management_price
    ) public {
        require(min_checking_interval > 14 && management_price > 0/*, "invalid data"*/);
        Job storage job = jobs[job_id];
        check_is_our_draft(job);

        job.min_checking_interval = min_checking_interval;
        job.management_price = management_price;
    }

    // Reverts if there is no job corresponding to `job_id`.
    function get_sender_nonce(bytes32 job_id) public view returns (address, uint128) {
        Job storage job = jobs[job_id];
        require(job.non_null/*, "unknown job"*/);
        return (job.sender, job.nonce);
    }

    function is_job_non_null(bytes32 job_id) public view returns (bool) {
        return jobs[job_id].non_null;
    }

    function is_task_non_null(bytes32 task_id) public view returns (bool) {
        return tasks[task_id].non_null;
    }

    function is_draft(bytes32 job_id) public view returns (bool) {
        Job storage job = jobs[job_id];
        require(job.non_null/*, "unknown job"*/);
        return job.is_draft;
    }

    // Reverts if there is no draft job corresponding to `id`.
    function delete_draft(bytes32 job_id) public {
        Job storage job = jobs[job_id];
        check_is_our_draft(job);

        job.non_null = false; // just in case...
        delete jobs[job_id];
    }

    function lock(bytes32 job_id) public {
        Job storage job = jobs[job_id];
        check_is_our_draft(job);
        require(is_draft_ready(job)/*, "conditions unmet"*/);
        job.is_draft = false;

        for (uint128 i; i < job.arguments.length ; i++) {
            bytes32 task_id = calc_task_id(job_id, i/*, job.arguments[i]*/);
            require(tasks[task_id].non_null == false/*, "task collision"*/);
            tasks[task_id] = Task(job_id,
                                   i,
                                   TaskState.Incomplete,
                                   new address[](0),
                                   new bytes(0),
                                   /*
                                   new address[](0),
                                   new uint64[](0),
                                   new uint64[](0),
                                   */
                                   TaskDefiniteErrorKind.TimedOut, // Don't care of the value
                                   true);
            emit TaskPending(task_id);
        }

        uint max_price = calc_max_price(job);
        users[msg.sender].pending_money -= max_price;
        users[msg.sender].locked_money += max_price;
        emit PendingMoneyChanged(msg.sender, users[msg.sender].pending_money);
    }

    // ------------------------------------
    // Functions which require proper consensus.

    function push_manager(bytes32 task_id, address manager) public {
        require(tasks[task_id].non_null/*, "unknown task"*/);
        require(msg.sender == oracle/*, "only the oracle can do that"*/);
        require(tasks[task_id].state == TaskState.Incomplete/*, "task already complete or failed"*/);
        require(tasks[task_id].managers_addresses.length < 4 + jobs[tasks[task_id].job_id].timeout - jobs[tasks[task_id].job_id].min_checking_interval/*, too many managers registered*/);

        tasks[task_id].managers_addresses.push(manager);
    }

    function set_definitely_failed(bytes32 task_id, TaskDefiniteErrorKind reason) public {
        require(msg.sender == oracle/*, "only the oracle can do that"*/);
        Task storage task = tasks[task_id];
        require(task.non_null/*, "unknown task"*/);
        require(task.state == TaskState.Incomplete/*, "task already complete or failed"*/);

        task.state = TaskState.DefinetelyFailed;
        task.reason = reason;

        Job storage job = jobs[task.job_id];
        uint max_price = calc_max_price_per_task(job);
        users[job.sender].locked_money -= max_price;
        users[job.sender].pending_money += max_price - pay_managers(task);

        emit TaskDefinetelyFailed(task_id, reason);
        if (is_job_completed(task.job_id)) {
            emit JobCompleted(task.job_id);
        }
    }

    // TODO: be careful of maximum data which can be sent here...
    function set_completed(bytes32 task_id, bytes memory result, address[] memory workers_addresses, uint64[] memory worker_prices, uint64[] memory network_prices) public {
        require(msg.sender == oracle/*, "only the oracle can do that"*/);
        // TODO: really check everything or trust oracle ?
        Task storage task = tasks[task_id];
        require(task.non_null/*, "unknown task"*/);
        require(task.state == TaskState.Incomplete/*, "task already complete or failed"*/);
        require(task.result.length == 0/*, "already completed task"*/);
        require(result.length > 0/*, "empty result"*/);
        require(workers_addresses.length == worker_prices.length && workers_addresses.length == network_prices.length/*, "not same sizes"*/);
        require(workers_addresses.length == jobs[task.job_id].redundancy/*, "incorrect length"*/);

        task.state = TaskState.Completed;
        task.result = result;
        /*
        task.workers_addresses = workers_addresses;
        task.worker_prices = worker_prices;
        task.network_prices = network_prices;
        */

        Job storage job = jobs[task.job_id];
        uint total_actual_cost = 0;
        for (uint64 i ; i < workers_addresses.length ; i++) {
            uint actual_cost = worker_prices[i] * job.timeout
               + network_prices[i] * job.max_network_usage;

            users[workers_addresses[i]].pending_money += actual_cost;
            total_actual_cost += actual_cost;
            emit PendingMoneyChanged(workers_addresses[i], users[workers_addresses[i]].pending_money);
        }
        total_actual_cost += pay_managers(task);
        uint max_price = calc_max_price_per_task(job);
        users[job.sender].locked_money -= max_price;
        users[job.sender].pending_money += max_price - total_actual_cost;

        emit PendingMoneyChanged(job.sender, users[job.sender].pending_money);

        emit TaskCompleted(task_id, result);
        if (is_job_completed(task.job_id)) {
            emit JobCompleted(task.job_id);
        }
    }

    // Returns the state of the task.
    // If it's Complete, only the result, the last value is relevent.
    // If it's DefinetelyFailed, only the failure reason, the second value is relevent.
    // If it's Incomplete, none of the other values mean anything.
    // Reverts if there is no task corresponding to `task_id`.
    function get_task_state(bytes32 task_id) public view returns (TaskState, TaskDefiniteErrorKind, bytes memory) {
        Task storage task = tasks[task_id];
        require(task.non_null/*, "unknown task"*/);
        return (task.state, task.reason, task.result);
    }

    // Reverts if there is no task corresponding to `task_id`.
    function get_task(bytes32 task_id) public view returns (bytes32, uint128) {
        Task storage task = tasks[task_id];
        require(task.non_null/*, "unknown task"*/);
        return (
            task.job_id,
            task.argument_id
        );
    }

    function get_argument(bytes32 task_id) public view returns (bytes memory) {
        Task storage task = tasks[task_id];
        require(task.non_null/*, "unknown task"*/);
        Job storage job = jobs[task.job_id];
        require(job.non_null/*, "unknown job"*/);
        require(job.arguments.length > task.argument_id/*, "unknown argument id"*/);

        return job.arguments[task.argument_id];
    }

    // ------------------------------------------------------------
    // Events

    event JobNew(address sender, uint128 nonce);
    event TaskPending(bytes32 task_id);
    event TaskCompleted(bytes32 task_id, bytes result);
    event TaskDefinetelyFailed(bytes32 task_id, TaskDefiniteErrorKind reason);
    event PendingMoneyChanged(address account, uint new_val);
    event JobCompleted(bytes32 job_id);
}
