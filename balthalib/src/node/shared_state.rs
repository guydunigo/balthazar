// TODO: review all in respect to specs and all...
use super::{Balthazar, LogKind};

use misc::{
    job::{try_bytes_to_address, Address, TaskId},
    shared_state::{PeerId, SharedState, SubTasksState, Task, TaskCompleteness, WorkerPaymentInfo},
};
use proto::{
    manager as man,
    manager::TaskDefiniteErrorKind,
    worker::{self, TaskErrorKind},
};
use std::{cmp::Ordering, time::SystemTime};

/// Events created when the shared state is modified...
#[derive(Debug, Clone)]
pub enum Event {
    /// A subtask is now pending and ready to be assigned.
    Pending,
    /// A subtask was assigned successfuly to given worker.
    Assigned { worker: PeerId },
    /// Because of failure or something, the given worker should stop working it.
    Unassigned {
        /// PeerId of worker executing the task.
        worker: PeerId,
        /// PeerId of its manager to notify it.
        workers_manager: PeerId,
    },
}

/// Defines changes which should be operated on state.
#[derive(Clone, Debug, PartialEq, Eq)]
// TODO: delete task to free memory?
enum StateChange {
    Create {
        redundancy: u64,
    },
    Checked {
        worker: PeerId,
    },
    Unassign {
        worker: PeerId,
    },
    Assign {
        worker: PeerId,
        workers_manager: PeerId,
        payment_info: WorkerPaymentInfo,
    },
    AddManager,
    SetNbFailures(u64),
    Complete {
        result: Vec<u8>,
    },
    DefinetelyFailed {
        reason: TaskDefiniteErrorKind,
    },
}

impl StateChange {
    /// Defines the order the changes must be executed to ensure less errors.
    fn to_cmp_int(&self) -> u8 {
        match self {
            StateChange::Create { .. } => 0,
            StateChange::Checked { .. } => 1,
            StateChange::Unassign { .. } => 2,
            StateChange::Assign { .. } => 3,
            StateChange::AddManager => 4,
            StateChange::SetNbFailures { .. } => 5,
            StateChange::Complete { .. } => 6,
            StateChange::DefinetelyFailed { .. } => 7,
        }
    }
}

impl Ord for StateChange {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_int = self.to_cmp_int();
        let other_int = other.to_cmp_int();
        self_int.cmp(&other_int)
    }
}

impl PartialOrd for StateChange {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn check_task_is_known<'a>(
    shared_state: &'a impl std::ops::Deref<Target = SharedState>,
    task_id: &TaskId,
) -> Result<&'a Task, String> {
    shared_state
        .tasks
        .get(task_id)
        .ok_or_else(|| "Task isn't known.".to_string())
}

fn get_substates(task: &Task) -> Result<&[SubTasksState], String> {
    task.get_substates()
        .ok_or_else(|| "Task no more incomplete.".to_string())
}
/*
fn get_substates_mut(task: &mut Task) -> Result<&mut [SubTasksState], String> {
    task.get_substates_mut()
        .ok_or_else(|| "Task no more incomplete.".to_string())
}
*/

impl Balthazar {
    pub async fn log_shared_state(&self, msg: String) {
        self.spawn_log(LogKind::SharedState, msg).await;
    }
    async fn log_res(&self, res: Result<Vec<StateChange>, String>, name: &str) -> Vec<StateChange> {
        let (msg, actions) = match res {
            Ok(mut actions) => {
                actions.push(StateChange::AddManager);
                (format!("{}: accepted.", name), actions)
            }
            Err(err) => (format!("Error: {}: {}", name, err), Vec::new()),
        };
        self.log_shared_state(msg).await;
        actions
    }

    async fn apply_state_change(
        &self,
        shared_state: &mut impl std::ops::DerefMut<Target = SharedState>,
        task_id: TaskId,
        workers_address: Address,
        action: StateChange,
    ) -> Result<(), String> {
        if let StateChange::Create { redundancy } = action {
            // TODO: check it wasn't existing ? No it isn't supposed to...
            shared_state
                .tasks
                .insert(task_id.clone(), Task::new(task_id.clone(), redundancy));

            for _ in 0..redundancy {
                self.spawn_shared_state_change(task_id.clone(), Event::Pending)
                    .await;
            }
            return Ok(());
        }

        let task = shared_state
            .tasks
            .get_mut(&task_id)
            .expect("Still no task, but we aren't supposed to reach here...");

        if !task.is_incomplete() {
            return Err("Task is already complete, can change anything about it!".to_string());
        }

        match action {
            StateChange::Create { .. } => (),
            StateChange::Checked { worker } => {
                let substate = task.get_substate_mut(&worker).ok_or_else(|| {
                    "Worker not assigned to task, but this is supposed to be checked before."
                        .to_string()
                })?;
                substate.update_last_check_timestamp();
            }
            StateChange::AddManager => {
                task.managers_addresses_mut().push(workers_address);
            }
            StateChange::Unassign { worker } => {
                if let Some(assigned) = task.unassign(&worker) {
                    let (worker, workers_manager) = assigned.into_unassigned();
                    self.spawn_shared_state_change(
                        task_id.clone(),
                        Event::Unassigned {
                            worker,
                            workers_manager,
                        },
                    )
                    .await;
                    self.spawn_shared_state_change(task_id, Event::Pending)
                        .await;
                } else {
                    return Err(
                        "Worker not assigned to task, but this is supposed to be checked before."
                            .to_string(),
                    );
                }
            }
            StateChange::Assign {
                worker,
                workers_manager,
                payment_info,
            } => {
                if let Err(msg) = task.assign(worker.clone(), workers_manager, payment_info) {
                    return Err(format!(
                        "Normally already checked before deciding action: {}",
                        msg
                    ));
                }
                self.spawn_shared_state_change(task_id, Event::Assigned { worker })
                    .await;
            }
            StateChange::SetNbFailures(nb) => {
                task.set_nb_failures(nb);
            }
            StateChange::Complete { result } => {
                task.set_completed(result);
                let (result, payment_info) = if let TaskCompleteness::Completed {
                    result,
                    workers_payment_info,
                } = task.completeness()
                {
                    (&result[..], &workers_payment_info[..])
                } else {
                    unreachable!("Just set.");
                };

                self.chain()
                    .jobs_set_managers(&task_id, task.managers_addresses())
                    .await
                    .map_err(|err| format!("Couldn't set managers on the Jobs SC: {}", err))?;
                self.chain()
                    .jobs_set_completed(&task_id, result, payment_info)
                    .await
                    .map_err(|err| format!("Couldn't set completed on the Jobs SC: {}", err))?;
            }
            StateChange::DefinetelyFailed { reason } => {
                task.set_definitely_failed(reason);
                let chain = self.chain();
                chain
                    .jobs_set_managers(&task_id, task.managers_addresses())
                    .await
                    .map_err(|err| format!("Problem setting managers in the Jobs SC: {}", err))?;
                chain
                    .jobs_set_definitely_failed(&task_id, reason)
                    .await
                    .map_err(|err| {
                        format!(
                            "Problem setting task as definitely failed in the Jobs SC: {}",
                            err
                        )
                    })?;
            }
        }

        Ok(())
    }

    // Accepting everything...
    // TODO: Channel out to send notif to wake up other parts ?
    // TODO: check correctness of proof messages, or is it done earlier ?
    // TODO: check we received them...
    pub async fn check_and_apply_proposal(&self, proposal: man::Proposal) {
        let mut shared_state = self.shared_state.write().await;

        let payment_address = match try_bytes_to_address(&proposal.payment_address[..]) {
            Ok(addr) => addr,
            Err(err) => {
                return self
                    .log_shared_state(format!("Could not parse payment address: `{}`.", err))
                    .await;
            }
        };

        let task_id = match TaskId::from_bytes(proposal.task_id) {
            Ok(task_id) => task_id,
            Err(err) => {
                return self
                    .log_shared_state(format!("Could not parse task id in proposal: `{}`.", err))
                    .await;
            }
        };

        // Is the proposal accepted and registered ?
        let (actions_res, name) = match proposal.proposal {
            Some(man::ProposalKind::NewTask(p)) => (
                self.handle_proposal_new_task(&shared_state, &task_id, p)
                    .await,
                "ProposeNewTask",
            ),
            Some(man::ProposalKind::Failure(p)) => (
                self.handle_proposal_failure(&shared_state, &task_id, p)
                    .await,
                "ProposeFailure",
            ),
            Some(man::ProposalKind::Scheduling(p)) => (
                self.handle_proposal_scheduling(&shared_state, &task_id, p)
                    .await,
                "ProposeScheduling",
            ),
            Some(man::ProposalKind::Checked(p)) => (
                self.handle_proposal_checked(&shared_state, &task_id, p)
                    .await,
                "ProposeChecked",
            ),
            Some(man::ProposalKind::Completed(p)) => (
                self.handle_proposal_completed(&shared_state, &task_id, p)
                    .await,
                "ProposeCompleted",
            ),
            None => {
                return self.log_shared_state("Empty proposal...".to_string()).await;
            }
        };

        let mut actions = self.log_res(actions_res, name).await;
        actions.sort();
        for action in actions.drain(..) {
            self.apply_state_change(&mut shared_state, task_id.clone(), payment_address, action)
                .await
                .unwrap()
        }
    }

    async fn handle_proposal_new_task(
        &self,
        shared_state: &impl std::ops::Deref<Target = SharedState>,
        task_id: &TaskId,
        _proposal: man::ProposeNewTask,
    ) -> Result<Vec<StateChange>, String> {
        if shared_state.tasks.contains_key(task_id) {
            Err("Task already known.".to_string())
        } else {
            let redundancy = {
                let chain = self.chain();
                // TODO: make it one call only...
                let (job_id, _) = chain.jobs_get_task(&task_id, true).await.map_err(|err| {
                    format!(
                        "Problem fetchin task `{}` from Jobs smart-contract: {}",
                        task_id, err
                    )
                })?;
                let (_, redundancy, _) =
                    chain
                        .jobs_get_parameters(&job_id, true)
                        .await
                        .map_err(|err| {
                            format!(
                            "Problem fetching redundancy for job `{}` in Jobs smart-contract: {}",
                            job_id, err
                        )
                        })?;
                redundancy
            };

            // Ok("Task registered.".to_string())
            Ok(vec![StateChange::Create { redundancy }])
        }
    }

    // TODO: Check all things sub-messages.
    async fn handle_proposal_failure(
        &self,
        shared_state: &impl std::ops::Deref<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeFailure,
    ) -> Result<Vec<StateChange>, String> {
        let task = check_task_is_known(shared_state, task_id)?;

        let max_failures = {
            let chain = self.chain();
            // TODO: make it one call only...
            let (job_id, _) = chain.jobs_get_task(&task_id, true).await.map_err(|err| {
                format!(
                    "Problem fetching task `{}` from Jobs smart-contract: {}",
                    task_id, err
                )
            })?;
            let (_, _, max_failures) =
                chain
                    .jobs_get_parameters(&job_id, true)
                    .await
                    .map_err(|err| {
                        format!(
                            "Problem fetching max_failures for job `{}` in Jobs smart-contract: {}",
                            job_id, err
                        )
                    })?;
            max_failures
        };

        // If worker_to_unassign is None, unassign all workers.
        let (reason, correct_val, worker_to_unassign) = match proposal.kind {
            // TODO: check task statuses and new_nb_failures...
            Some(man::FailureKind::Worker(man::ProposeFailureWorker {
                original_message:
                    Some(man::ManTaskStatus {
                        // TODO: got lazy in testing all parameters, but at the same time it was maybe
                        // already checked right beforehand...
                        worker,
                        status:
                            Some(worker::TaskStatus {
                                status_data: Some(worker::StatusData::Error(err_i32)),
                                ..
                            }),
                        ..
                    }),
                ..
            })) => {
                let error = worker::TaskErrorKind::from_i32(err_i32)
                    .ok_or_else(|| "Couldn't parse error type.".to_string())?;

                use TaskErrorKind::*;

                let is_nb_failure_correct = match error {
                    Aborted => proposal.new_nb_failures == task.nb_failures(),
                    TimedOut | Download | Runtime => {
                        proposal.new_nb_failures == task.nb_failures() + 1
                    }
                    _ => {
                        return Err("Incorrect error kind in TaskStatus.".to_string());
                    }
                };
                let error = man::try_worker_error_to_definite_error(error);
                (error, is_nb_failure_correct, Some(worker))
            }
            Some(man::FailureKind::Worker(_)) => {
                return Err("Invalid or missing original_message.".to_string());
            }
            Some(man::FailureKind::ManUnavailable(man::ProposeFailureManagerUnavailable {
                unanswered_ping:
                    Some(man::PingManagerForTask {
                        task_id: task_id_2,
                        worker,
                        ..
                    }),
            })) => {
                if &task_id_2[..] != task_id.as_bytes() {
                    return Err("Ping for wrong task.".to_string());
                }

                (
                    None,
                    proposal.new_nb_failures == task.nb_failures(),
                    Some(worker),
                )
            }
            Some(man::FailureKind::ManUnavailable(_)) => {
                return Err("Missing unanswered_ping.".to_string());
            }
            Some(man::FailureKind::Results(_)) => (
                Some(TaskDefiniteErrorKind::IncorrectResult),
                proposal.new_nb_failures == task.nb_failures() + 1,
                None,
            ),
            Some(man::FailureKind::Specs(_)) => (
                Some(TaskDefiniteErrorKind::IncorrectSpecification),
                proposal.new_nb_failures >= max_failures,
                None,
            ),
            None => {
                return Err("Missing proposal failure kind.".to_string());
            }
        };

        if !correct_val {
            return Err("Error: ProposeFailure: bad new number of failures".to_string());
        }

        // TODO: store list of failures rather than only last one?
        let mut actions = vec![StateChange::SetNbFailures(proposal.new_nb_failures)];

        if worker_to_unassign.is_none() || task.nb_failures() >= max_failures {
            task.get_substates()
                .ok_or_else(|| "Not in incomplete.".to_string())?
                .iter()
                .filter_map(|a| a.as_ref())
                .for_each(|a| {
                    actions.push(StateChange::Unassign {
                        worker: a.worker().clone(),
                    })
                });
        } else if let Some(worker) = worker_to_unassign {
            let worker = PeerId::from_bytes(worker)
                .map_err(|_| "Couln't parse worker PeerId.".to_string())?;
            if task.get_substate(&worker).is_some() {
                actions.push(StateChange::Unassign { worker });
            } else {
                return Err("Worker not assigned to it.".to_string());
            }
        }

        // Definetely failed if too many failures:
        if proposal.new_nb_failures >= max_failures {
            if let Some(reason) = reason {
                actions.push(StateChange::DefinetelyFailed { reason })
            } else {
                panic!("`reason` is `None` but the `new_nb_failures` is too high and indicates a definitive failure, that should already be checked though.");
            }
        }

        // Ok("Definitely failed, set as such in the Jobs SC.".to_string())
        // Ok("Failed.".to_string())
        Ok(actions)
    }

    // TODO: check offers and all + no two twice the same worker...
    async fn handle_proposal_scheduling(
        &self,
        shared_state: &impl std::ops::Deref<Target = SharedState>,
        task_id: &TaskId,
        mut proposal: man::ProposeScheduling,
    ) -> Result<Vec<StateChange>, String> {
        let task = check_task_is_known(shared_state, task_id)?;

        let nb_unassigned = task
            .completeness()
            .get_nb_unassigned()
            .expect("We should have already checked this by now.");
        let mut offers_filtered = Vec::new();
        for o in proposal.selected_offers.drain(..).take(nb_unassigned) {
            if &o.task_id[..] != task_id.as_bytes() {
                return Err("Unmatching task id in offer.".to_string());
            }

            let payment_address = try_bytes_to_address(&o.payment_address[..])
                .map_err(|_| "Can't parse payment_address in offer.".to_string())?;
            // TODO: check also prices values...

            let worker = PeerId::from_bytes(o.worker)
                .map_err(|_| "Couln't parse worker PeerId.".to_string())?;

            if task.get_substate(&worker).is_some() {
                return Err("Worker already assigned to this task.".to_string());
            }

            let workers_manager = PeerId::from_bytes(o.workers_manager)
                .map_err(|_| "Couln't parse worker's manager PeerId.".to_string())?;

            offers_filtered.push(StateChange::Assign {
                worker,
                workers_manager,
                payment_info: WorkerPaymentInfo::new(
                    payment_address,
                    o.worker_price,
                    o.network_price,
                ),
            });
        }

        // Ok(format!("Stored {} tasks.", nb_assigned))
        Ok(offers_filtered)
    }

    // TODO: check pinging message and sender
    async fn handle_proposal_checked(
        &self,
        shared_state: &impl std::ops::Deref<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeCheckedRunning,
    ) -> Result<Vec<StateChange>, String> {
        let task = check_task_is_known(shared_state, task_id)?;
        let worker = PeerId::from_bytes(proposal.worker)
            .map_err(|_| "Couln't parse worker PeerId.".to_string())?;

        if let Some(substate) = task.get_substate(&worker) {
            let min_check_interval = {
                let chain = self.chain();
                // TODO: make it one call only...
                let (job_id, _) = chain.jobs_get_task(&task_id, true).await.map_err(|err| {
                    format!(
                        "Problem fetchin task `{}` from Jobs smart-contract: {}",
                        task_id, err
                    )
                })?;
                let (min_check_interval, _) =
                    chain
                        .jobs_get_management_parameters(&job_id, true)
                        .await
                        .map_err(|err| {
                            format!(
                            "Problem fetching min_check_interval for job `{}` in Jobs smart-contract: {}",
                            job_id, err
                        )
                        })?;
                min_check_interval
            };
            let duration = SystemTime::now()
                .duration_since(substate.last_check_timestamp())
                .map_err(|err| format!("Error checking duration since last check: {}", err))?;
            if duration.as_secs() < min_check_interval {
                return Err(format!(
                    "Too early of {} seconds.",
                    (min_check_interval - duration.as_secs())
                ));
            }

            // Ok("Last checked timestamp updated.".to_string())
            Ok(vec![StateChange::Checked { worker }])
        } else {
            Err("Worker not assigned to it.".to_string())
        }
    }

    // TODO: check completion signals...
    // TODO: check that selected_result is part of other signals...
    async fn handle_proposal_completed(
        &self,
        shared_state: &impl std::ops::Deref<Target = SharedState>,
        task_id: &TaskId,
        proposal: man::ProposeCompleted,
    ) -> Result<Vec<StateChange>, String> {
        let task = check_task_is_known(shared_state, task_id)?;
        if !task.is_incomplete() {
            return Err("Task already no more incomplete.".to_string());
        }

        let substates = get_substates(&task)?;
        let (task_id_2, worker, status) = if let Some(man::ManTaskStatus {
            task_id: task_id_2,
            worker,
            status: Some(status),
        }) = proposal.selected_result
        {
            let worker = PeerId::from_bytes(worker)
                .map_err(|_| "Couln't parse worker PeerId.".to_string())?;
            (task_id_2, worker, status)
        } else {
            return Err("Empty selected_result or status.".to_string());
        };

        if &task_id_2[..] != task_id.as_bytes() {
            return Err("ManTaskStatus not corresponding to this task.".to_string());
        }

        // TODO: already checked when receiving it ?
        if substates
            .iter()
            .all(|s| s.as_ref().filter(|a| *a.worker() == worker).is_none())
        {
            return Err("ManTaskStatus worker not assigned to this task.".to_string());
        }

        // TODO: already checked when receiving it ?
        let (task_id_3, result) = if let worker::TaskStatus {
            task_id: task_id_3,
            status_data: Some(worker::StatusData::Result(result)),
        } = status
        {
            (task_id_3, result)
        } else {
            return Err("No status data or incorrect one.".to_string());
        };

        // TODO: already checked when receiving it ?
        if &task_id_3[..] != task_id.as_bytes() {
            return Err("TaskStatus not corresponding to this task.".to_string());
        }

        Ok(vec![StateChange::Complete { result }])
    }
}
