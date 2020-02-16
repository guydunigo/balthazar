# Protocol

The communication being designed as request-answer, there should always be a response to any request.
In case where there is no need for a specific answer, `Ack` should be sent back.

Here are the different requests followed by their expected answers (answers are marked with a `>`).

## Messages

| Message (from A to B) | Description |
| --- | --- | --- |
| NodeTypeRequest { node_type: NodeType } | Requests the node type of B and advertise its own node type |
| > NodeTypeAnswer { node_type: NodeType } | Answer to a `NodeTypeRequest`, with the type of A |
| | **What happens if the node type of a peer changes between two calls ?** |

| **Manager<->Worker** ||
| > NotMine | answer to any request specific to a manager/worker relationship, if A doesn't consider B as its worker or manager |
| | - if B considers A as its worker or manager, B should sending to A any more manager/worker-relationship specific messages (see ManagerBye) |

| ManagerRequest { cores: uint, memory: uint, ... **TBD** } | Requests if B can manage A, A providing its own specifications |
| | - A must be a worker, B a manager |
| | - B must be authorised to be A's manager (via node configuration) |
| | - if A already has a manager, it should not send this message to anyone before unsubscribing to its current one (see ManagerBye) |
| | **How much can we thrust the specs sent by A ?** |
| > ManagerAnswer { accepted: Bool } | - if A manager, B worker and accepted is `true`, A becomes the manager of B, and they will respond only to each other for manager/worker related messages |
| | - if A manager and B worker are already in a relationship, the answer shouldn't be considered (but can trigger a PingManager to check on the status) |
| | - if A manager, B worker and accepted is `false`, there is no guarante that A will always answer `false` for B |
| | - if A is a manager, decides and returns the answer |
| | - if A is a worker, send `false` |
| | - if B isn't known as a worker, send `false`  |
| | - if A accepted but for some reason, B can't be A's worker: B should send a `ManagerBye`, those reasons include: |
| | --> if B is a worker, but already has another manager (that is not A) |
| | --> if B is a manager |
| | --> if B isn't allowed to be managed by A |
| | --> if A's type is unknown or know as a worker |

| ManagerBye | Acknowledge B that A stops acting as B's worker or manager. |
| | - if A isn't B's manager or worker, does nothing. |
| | - if they were, they should stop sending each other manager/worker specific messages |
| > Ack | Expected answer, whatever the relationship was between A and B |

| ManagerPing | Sent when A wants to check worker/manager relationship with B |
| | - A should consider B as its worker/manager |
| > ManagerPong | A and B can consider themselves as still in a relationship |
| > NotMine | if A isn't (or is no more) in relationship with B (see NotMine above) |

| TasksExecute { tasks: [(job_addr, arguments, timeout, **maybe other?**)] } | Asks B to execute given tasks |
| | A must be B's manager |
| > Ack | |
| > **TODO** | Refuses to work on the given tasks |
| > NotMine | if B isn't A's manager (see NotMine above) |

| TasksPing { tasks: [TaskId] } | manager A asks about status of tasks on worker B |
| | A must be B's manager |
| > TasksPong { tasks_status: [(TaskId, TaskStatus)] } | every tasks in the TasksPing should have an entry here, not necessarily in the same order |
| > NotMine | if B isn't A's manager (see NotMine above) |

| TasksAbord { tasks: [TaskId }] | Manager A asks its worker B to stop working on tasks |
| | Both should stop talking about these tasks to each other |
| | for each task, if it was started, kill it otherwise remove it from B's pending tasks |
| | Note: this can be used when a task is long overdue but the worker hasn't sent a TimeOut error |
| | A must be B's manager |
| > Ack | |
| > NotMine | if B isn't A's manager (see NotMine above) |

| TaskStarted { task_id: TaskId, datetime: **type?** } | worker A notifies his manager B: task task_id has been started |
| | Note: both worker and manager should track time of task to know if timeout happened |
| | A must be B's worker |
| > Ack | |
| > NotMine | if A isn't B's manager (see NotMine above) |

| TaskError { task_id: TaskId, error: ErrorKind } | worker A notifies his manager B: an error occured when trying to execute task task_id |
| | A must be B's worker |
| > Ack | |
| > NotMine | if A isn't B's manager (see NotMine above) |

| TaskCompleted { task_id: TaskId, result: **result or address to result data?** } | worker A notifies his manager B: task task_id has been completed and gave result |
| | A must be B's worker |
| > Ack | |
| > NotMine | if A isn't B's manager (see NotMine above) |

---
## Enumerators and types

```rust
enum NodeType {
  Manager,
  Worker,
}
```

```rust
enum TaskStatus {
  Pending, // task is waiting to start
  Started(datetime: **type?**), // task has been started on `date`
  Error(ErrorKind), // something occured preventing a task to be executed successfuly
  Completed(result **or link to result?**), // task was completed
  Unknown, // worker doesn't know this task
}
```

```rust
enum ErrorKind {
  TimedOut,
  DownloadError,
  RunningError,
  ... // others ?
}
```
