use misc::{job::TaskId, shared_state::PeerId};
use std::cmp::Ordering;

// TODO: place Pending at the end of the list...

#[derive(Debug, Clone)]
pub enum WorkerAssignment {
    /// Is free to perform any work.
    Available,
    /// Is waiting for confirmation on a proposal.
    Pending, // add pending for task ?
    /// Is currently set to perform given task.
    Assigned(TaskId),
}

impl Default for WorkerAssignment {
    fn default() -> Self {
        WorkerAssignment::Available
    }
}

impl WorkerAssignment {
    pub fn is_available(&self) -> bool {
        if let WorkerAssignment::Available = self {
            true
        } else {
            false
        }
    }

    pub fn is_pending(&self) -> bool {
        if let WorkerAssignment::Pending = self {
            true
        } else {
            false
        }
    }

    /*
    pub fn is_assigned(&self) -> bool {
        if let WorkerAssignment::Assigned(_) = self {
            true
        } else {
            false
        }
    }
    */
}

#[derive(Debug, Clone)]
pub struct Worker {
    peer_id: PeerId,
    assignments: Vec<WorkerAssignment>,
    // TODO: workers specs
}

impl Worker {
    pub fn new(peer_id: PeerId, cpu_count: u64) -> Self {
        let mut assignments = Vec::with_capacity(cpu_count as usize);
        assignments.resize_with(cpu_count as usize, Default::default);
        Worker {
            peer_id,
            assignments,
        }
    }

    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn assignments(&self) -> &[WorkerAssignment] {
        &self.assignments[..]
    }

    pub fn assignments_mut(&mut self) -> &mut [WorkerAssignment] {
        &mut self.assignments[..]
    }

    pub fn into_assignments(self) -> Vec<WorkerAssignment> {
        self.assignments
    }

    /*
    pub fn get_nb_slots(&self) -> usize {
        self.assignments().len()
    }
    */

    pub fn get_nb_available_slots(&self) -> usize {
        self.assignments()
            .iter()
            .filter(|a| a.is_available())
            .count()
    }

    pub fn get_nb_pending_slots(&self) -> usize {
        self.assignments().iter().filter(|a| a.is_pending()).count()
    }

    /*
    pub fn get_nb_assigned_slots(&self) -> usize {
        self.assignments()
            .iter()
            .filter(|a| a.is_assigned())
            .count()
    }
    */

    /// Sets an [`WorkerAssignment::Available`] to a [`WorkerAssignment::Pending`]
    /// and returns [`true`].
    /// If there's no more available slot, returns [`false`]
    pub fn reserve_slot(&mut self) -> bool {
        if let Some(a) = self.assignments_mut().iter_mut().find(|a| a.is_available()) {
            *a = WorkerAssignment::Pending;
            true
        } else {
            false
        }
    }

    /*
    /// Sets an [`WorkerAssignment::Pending`] to a [`WorkerAssignment::Available`]
    /// and returns [`true`].
    /// If there's no more available slot, returns [`false`]
    pub fn unreserve_slot(&mut self) -> bool {
        if let Some(a) = self.assignments_mut().iter_mut().find(|a| a.is_pending()) {
            *a = WorkerAssignment::Available;
            true
        } else {
            false
        }
    }
    */

    /// Assign an empty slot to a task.
    /// Tries first to take a [`WorkerAssignment::Pending`],
    /// and then take tries to find a [`WorkerAssignment::Available`].
    /// If it could find a slot, returns [`true`], otherwise returns [`false`].
    pub fn assign_slot(&mut self, task_id: TaskId) -> bool {
        if let Some(a) = self.assignments_mut().iter_mut().find(|a| a.is_pending()) {
            *a = WorkerAssignment::Assigned(task_id);
            true
        } else if let Some(a) = self.assignments_mut().iter_mut().find(|a| a.is_available()) {
            *a = WorkerAssignment::Assigned(task_id);
            true
        } else {
            false
        }
    }

    /// Sets an [`WorkerAssignment::Assign`] to a [`WorkerAssignment::Available`]
    /// and returns [`true`].
    /// If there's no more available slot, returns [`false`]
    pub fn unassign_slot(&mut self, task_id: &TaskId) -> bool {
        if let Some(a) = self.assignments_mut().iter_mut().find(|a| {
            if let WorkerAssignment::Assigned(a_task_id) = a {
                *a_task_id == *task_id
            } else {
                false
            }
        }) {
            *a = WorkerAssignment::Available;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Workers {
    workers: Vec<Worker>,
}

impl Workers {
    pub fn push(&mut self, peer_id: PeerId, cpu_count: u64) {
        if self.get_worker(&peer_id).is_none() {
            self.workers.push(Worker::new(peer_id, cpu_count));
        } else {
            // TODO: or panic! ?
            eprintln!("Worker already known!");
        }
    }

    pub fn remove(&mut self, peer_id: &PeerId) -> Option<Vec<WorkerAssignment>> {
        if let Some(index) = self.workers.iter().enumerate().find_map(|(index, w)| {
            if *w.peer_id() == *peer_id {
                Some(index)
            } else {
                None
            }
        }) {
            Some(self.workers.remove(index).into_assignments())
        } else {
            // TODO: or panic! ?
            eprintln!("Unknown worker!");
            None
        }
    }

    pub fn get_worker(&self, peer_id: &PeerId) -> Option<&Worker> {
        self.workers.iter().find(|w| *w.peer_id() == *peer_id)
    }

    pub fn get_worker_mut(&mut self, peer_id: &PeerId) -> Option<&mut Worker> {
        self.workers.iter_mut().find(|w| *w.peer_id() == *peer_id)
    }

    /// Find workers wich have a slot not in [`WorkerAssignment::Assigned`] state.
    // TODO: return Iterator?
    pub fn get_unassigned_workers(&self) -> Vec<&PeerId> {
        self.workers
            .iter()
            .filter(|w| w.get_nb_available_slots() > 0 || w.get_nb_pending_slots() > 0)
            .map(|w| w.peer_id())
            .collect()
    }

    /// Find workers wich have a slot not in [`WorkerAssignment::Assigned`] state,
    /// placing the ones having [`WorkerAssignment::Available`] slots before the
    /// ones in having only [`WorkerAssignment::Pending`] ones.
    // TODO: more complex using nb assigned and all...
    // TODO: return Iterator?
    pub fn get_unassigned_workers_sorted(&self) -> Vec<&PeerId> {
        let mut workers = self.get_unassigned_workers();
        workers.sort_by(|a, b| {
            let w_a = self.get_worker(a).unwrap();
            let w_b = self.get_worker(b).unwrap();

            match (
                w_a.get_nb_available_slots() > 0,
                w_b.get_nb_available_slots() > 0,
            ) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                (false, false) => Ordering::Equal,
            }
        });
        workers
    }

    pub fn reserve_slot(&mut self, peer_id: &PeerId) -> bool {
        if let Some(val) = self.get_worker_mut(peer_id).map(|w| w.reserve_slot()) {
            val
        } else {
            false
        }
    }

    /*
    pub fn unreserve_slot(&mut self, peer_id: &PeerId) -> bool {
        if let Some(val) = self.get_worker_mut(peer_id).map(|w| w.unreserve_slot()) {
            val
        } else {
            false
        }
    }
    */

    pub fn assign_slot(&mut self, peer_id: &PeerId, task_id: TaskId) -> bool {
        if let Some(val) = self.get_worker_mut(peer_id).map(|w| w.assign_slot(task_id)) {
            val
        } else {
            false
        }
    }

    pub fn unassign_slot(&mut self, peer_id: &PeerId, task_id: &TaskId) -> bool {
        if let Some(val) = self
            .get_worker_mut(peer_id)
            .map(|w| w.unassign_slot(task_id))
        {
            val
        } else {
            false
        }
    }
}
