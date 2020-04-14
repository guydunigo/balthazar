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

    enum TaskErrorKind {
        IncorrectSpecification,
        TimedOut,
        DownloadError,
        Runtime,
        IncorrectResult,// ,
        // Aborted,
        Unknown
    }

    struct Job {
        bytes[] arguments;
        uint64 timeout;
        uint64 max_worker_price;
        uint64 max_network_usage;
        uint64 max_network_price;

        uint64 redundancy;
        uint64 max_failures;

        uint64 min_checking_interval;
        uint64 management_price;

        bytes data;

        address sender;
        uint128 nonce;

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

        TaskErrorKind reason;

        bool non_null;
    }

    struct User {
        // don't need to store that, just go through all nonces so all jobs for this user...
        // bytes32[] pending_jobs;
        // bytes32[] completed_jobs;
        Job[] draft_jobs;
        uint128 next_nonce;
        uint256 locked_money;
        uint256 pending_money;
    }

    mapping(bytes32 => Job) jobs;
    mapping(bytes32 => Task) tasks;
    mapping(address => User) users;

    // TODO: ability to set this address ?
    address oracle;

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

    function calc_task_id(bytes32 job_id, uint128 index, bytes storage argument) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(job_id, index, argument));
    }

    function is_draft_ready(Job storage job) internal view returns (bool) {
        // TODO: check if correct and add manager...
        return job.arguments.length > 0 &&

            job.timeout > 9 &&

            job.max_worker_price > 0 &&
            job.max_network_price > 0 &&
            job.redundancy > 0 &&
            job.min_checking_interval > 14 &&
            job.management_price > 0 &&

            users[msg.sender].pending_money >= calc_max_price(job);
    }

    // TODO: useful? or directly store drafts to usual jobs...
    function find_draft_program(uint128 nonce) internal view returns (uint) {
        for (uint i ; i < users[msg.sender].draft_jobs.length ; i++) {
            if (users[msg.sender].draft_jobs[i].nonce == nonce) {
                return i;
            }
        }
        revert("Not found.");
    }

    function pay_managers(Task storage task) internal {
        require(task.managers_addresses.length > 0/*, no managers set*/);
        Job storage job = jobs[task.job_id];

        for (uint64 i ; i < task.managers_addresses.length; i++) {
            users[task.managers_addresses[i]].pending_money += job.management_price;
            emit PendingMoneyChanged(task.managers_addresses[i], users[task.managers_addresses[i]].pending_money);
        }

        uint256 management_price = job.management_price * task.managers_addresses.length;
        users[job.sender].locked_money -= management_price;
        users[job.sender].pending_money += calc_max_price_per_task(job) - management_price;
    }

    // Reverts if there is no job corresponding to `job_id`.
    function is_job_completed(bytes32 job_id) public view returns (bool result) {
        require(jobs[job_id].non_null/*, "unknown job"*/);

        result = true;
        for (uint128 i ; i < jobs[job_id].arguments.length ; i++) {
            if (tasks[calc_task_id(job_id, i, jobs[job_id].arguments[i])].state == TaskState.Incomplete) {
                result = false;
                break;
            }
        }
    }

    // ------------------------------------
    // User functions

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

    function get_draft_jobs() public view returns (uint128[] memory list) {
        Job[] storage draft_jobs = users[msg.sender].draft_jobs;
        list = new uint128[](draft_jobs.length);
        for (uint i; i < draft_jobs.length; i++) {
            list[i] = draft_jobs[i].nonce;
        }
    }
    function get_pending_locked_money() public view returns (uint256, uint256) {
        return (users[msg.sender].pending_money, users[msg.sender].locked_money);
    }
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

    function create_job() public {
        uint128 nonce = next_nonce_and_increase();
        users[msg.sender].draft_jobs.push(Job(
            new bytes[](0),
            1,
            1,
            0,
            1,
            1,
            0,
            15,
            1,
            new bytes(0),
            msg.sender,
            nonce,
            true
        ));
        emit JobNew(msg.sender, nonce);
    }

    function get_parameters_draft(uint128 nonce) public view returns (
        // ProgramKind,
        uint64,
        uint64,
        uint64
    ) {
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        return (
            j.timeout,
            j.max_failures,
            j.redundancy
        );
    }
    // Reverts if `timeout` or `max_failures` is null.
    function set_parameters_draft(
        uint128 nonce,
        uint64 timeout,
        uint64 redundancy,
        uint64 max_failures//,
    ) public {
        require(timeout > 0 && redundancy > 0/*, "invalid data"*/);
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        j.timeout = timeout;
        j.redundancy = redundancy;
        j.max_failures = max_failures;
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_parameters(bytes32 job_id) public view returns (
        uint64,
        uint64,
        uint64
    ) {
        require(jobs[job_id].non_null/*, "unknown job"*/);
        return (
            jobs[job_id].timeout,
            jobs[job_id].redundancy,
            jobs[job_id].max_failures
        );
    }

    function get_data_draft(uint128 nonce) public view returns (bytes memory) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].data;
    }
    function set_data_draft(uint128 nonce, bytes memory val) public {
        users[msg.sender].draft_jobs[find_draft_program(nonce)].data = val;
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_data(bytes32 job_id) public view returns (bytes memory) {
        require(jobs[job_id].non_null/*, "unknown job"*/);
        return jobs[job_id].data;
    }

    function get_arguments_draft(uint128 nonce) public view returns (bytes[] memory) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].arguments;
    }
    // Reverts if the provided array is empty.
    function set_arguments_draft(uint128 nonce, bytes[] memory val) public {
        require(val.length > 0/*, "empty array"*/);
        users[msg.sender].draft_jobs[find_draft_program(nonce)].arguments = val;
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_arguments(bytes32 job_id) public view returns (bytes[] memory) {
        require(jobs[job_id].non_null/*, "unknown job"*/);
        return jobs[job_id].arguments;
    }

    function get_worker_parameters_draft(uint128 nonce) public view returns (
        uint64,
        uint64,
        uint64
    ) {
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        return (
            j.max_worker_price,
            j.max_network_usage,
            j.max_network_price
        );
    }
    // Reverts if `max_worker_price` or `max_network_price` is null.
    function set_worker_parameters_draft(
        uint128 nonce,
        uint64 max_worker_price,
        uint64 max_network_usage,
        uint64 max_network_price
    ) public {
        require(max_worker_price > 0 && max_network_price > 0/*, "invalid data"*/);
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        j.max_worker_price = max_worker_price;
        j.max_network_usage = max_network_usage;
        j.max_network_price = max_network_price;
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_worker_parameters(bytes32 job_id) public view returns (
        uint64,
        uint64,
        uint64
    ) {
        require(jobs[job_id].non_null/*, "unknown job"*/);

        return (
            jobs[job_id].max_worker_price,
            jobs[job_id].max_network_usage,
            jobs[job_id].max_network_price// ,
        );
    }

    function get_manager_parameters_draft(uint128 nonce) public view returns (
        uint64,
        uint64
    ) {
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        return (
            j.min_checking_interval,
            j.management_price
        );
    }
    // Reverts if `min_checking_interval` or `management_price` is null.
    function set_manager_parameters_draft(
        uint128 nonce,
        uint64 min_checking_interval,
        uint64 management_price
    ) public {
        require(min_checking_interval > 0 && management_price > 0/*, "invalid data"*/);
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        j.min_checking_interval = min_checking_interval;
        j.management_price = management_price;
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_manager_parameters(bytes32 job_id) public view returns (
        uint64,
        uint64
    ) {
        require(jobs[job_id].non_null/*, "unknown job"*/);

        return (
            jobs[job_id].min_checking_interval,
            jobs[job_id].management_price
        );
    }

    // Reverts if there is no job corresponding to `job_id`.
    function get_sender_nonce(bytes32 job_id) public view returns (address, uint128) {
        require(jobs[job_id].non_null/*, "unknown job"*/);
        return (jobs[job_id].sender, jobs[job_id].nonce);
    }

    // Reverts if there is no draft job corresponding to `id`.
    function delete_draft(uint id) internal returns (Job memory job) {
        User storage user = users[msg.sender];
        require(user.draft_jobs.length > id/*, "unknown id"*/);

        job = user.draft_jobs[id];

        if (user.draft_jobs.length > id + 1) {
            user.draft_jobs[id] = user.draft_jobs[user.draft_jobs.length - 1];
        }

        user.draft_jobs.pop();
    }

    function delete_draft_nonce(uint128 nonce) public {
        delete_draft(find_draft_program(nonce));
    }

    function ready(uint128 nonce) public {
        Job storage job = users[msg.sender].draft_jobs[find_draft_program(nonce)];
        require(is_draft_ready(job)/*, "conditions unmet"*/);

        bytes32 job_id = calc_job_id(job.sender, job.nonce);
        require(jobs[job_id].non_null == false/*, "job collision"*/);

        // TODO: a lot of copies ? Directly store the job in jobs from the beginning?
        jobs[job_id] = job;
        delete_draft(find_draft_program(nonce));
        job = jobs[job_id];

        for (uint128 i; i < job.arguments.length ; i++) {
            bytes32 task_id = calc_task_id(job_id, i, job.arguments[i]);
            require(tasks[task_id].non_null == false, "task collision");
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
                                   TaskErrorKind.Unknown,
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

    function set_managers(bytes32 task_id, address[] memory addresses) public {
        require(tasks[task_id].non_null/*, "unknown task"*/);
        require(msg.sender == oracle/*, "only the oracle can do that"*/);
        require(tasks[task_id].state == TaskState.Incomplete/*, "task already complete or failed"*/);
        require(addresses.length <= 4 + jobs[tasks[task_id].job_id].timeout - jobs[tasks[task_id].job_id].min_checking_interval/*, too many managers registered*/);

        tasks[task_id].managers_addresses = addresses;
    }

    function set_failed(bytes32 task_id, TaskErrorKind reason) public {
        require(msg.sender == oracle/*, "only the oracle can do that"*/);
        Task storage task = tasks[task_id];
        require(task.non_null/*, "unknown task"*/);
        require(task.state == TaskState.Incomplete/*, "task already complete or failed"*/);

        task.state = TaskState.DefinetelyFailed;
        task.reason = reason;
        pay_managers(task);

        emit TaskDefinetelyFailed(task_id, reason);
        if (is_job_completed(task.job_id)) {
            emit JobCompleted(task.job_id);
        }
    }

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
        users[job.sender].locked_money -= total_actual_cost;
        users[job.sender].pending_money += calc_max_price_per_task(job) - total_actual_cost;
        pay_managers(task);

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
    function get_state(bytes32 task_id) public view returns (TaskState, TaskErrorKind, bytes memory) {
        Task storage task = tasks[task_id];
        require(task.non_null/*, "unknown task"*/);
        return (task.state, task.reason, task.result);
    }

    // Reverts if there is no task corresponding to `task_id`.
    function get_task(bytes32 task_id) public view returns (bytes32, uint128, bytes memory) {
        Task storage task = tasks[task_id];
        require(task.non_null/*, "unknown task"*/);
        return (
            task.job_id,
            task.argument_id,
            jobs[task.job_id].arguments[task.argument_id]
        );
    }

    // ------------------------------------------------------------
    // Events

    event JobNew(address sender, uint128 nonce);
    event TaskPending(bytes32 task_id);
    event TaskCompleted(bytes32 task_id, bytes result);
    event TaskDefinetelyFailed(bytes32 task_id, TaskErrorKind reason);
    event PendingMoneyChanged(address account, uint new_val);
    event JobCompleted(bytes32 job_id);
}
