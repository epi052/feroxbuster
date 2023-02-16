use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::{atomic_load, atomic_store, config::RequesterPolicy};

use super::limit_heap::LimitHeap;

/// data regarding policy and metadata about last enforced trigger etc...
#[derive(Default, Debug)]
pub struct PolicyData {
    /// how to handle exceptional cases such as too many errors / 403s / 429s etc
    pub(super) policy: RequesterPolicy,

    /// whether or not we're in the middle of a cooldown period
    pub(super) cooling_down: AtomicBool,

    /// length of time to pause tuning after making an adjustment
    pub(super) wait_time: u64,

    /// rate limit (at last interval)
    limit: AtomicUsize,

    /// number of errors (at last interval)
    pub(super) errors: AtomicUsize,

    /// whether or not the owning Requester should remove the rate_limiter, happens when a scan
    /// has been limited and moves back up to the point of its original scan speed
    pub(super) remove_limit: AtomicBool,

    /// heap of values used for adjusting # of requests/second
    pub(super) heap: std::sync::RwLock<LimitHeap>,
}

/// implementation of PolicyData
impl PolicyData {
    /// given a RequesterPolicy, create a new PolicyData
    pub fn new(policy: RequesterPolicy, timeout: u64) -> Self {
        // can use this as a tweak for how aggressively adjustments should be made when tuning
        let wait_time = ((timeout as f64 / 2.0) * 1000.0) as u64;

        Self {
            policy,
            wait_time,
            ..Default::default()
        }
    }

    /// setter for requests / second; populates the underlying heap with values from req/sec seed
    pub(super) fn set_reqs_sec(&self, reqs_sec: usize) {
        if let Ok(mut guard) = self.heap.write() {
            guard.original = reqs_sec as i32;
            guard.build();
            self.set_limit(guard.inner[0] as usize); // set limit to 1/2 of current request rate
        }
    }

    /// setter for errors
    pub(super) fn set_errors(&self, errors: usize) {
        atomic_store!(self.errors, errors);
    }

    /// setter for limit
    fn set_limit(&self, limit: usize) {
        atomic_store!(self.limit, limit);
    }

    /// getter for limit
    pub(super) fn get_limit(&self) -> usize {
        atomic_load!(self.limit)
    }

    /// adjust the rate of requests per second up (increase rate)
    pub(super) fn adjust_up(&self, streak_counter: &usize) {
        if let Ok(mut heap) = self.heap.try_write() {
            if *streak_counter > 2 {
                // streak of 3 upward moves in a row, traverse the tree upward instead of to a
                // higher-valued branch lower in the tree
                let current = heap.value();
                heap.move_up();
                heap.move_up();
                if current > heap.value() {
                    // the tree's structure makes it so that sometimes 2 moves up results in a
                    // value greater than the current node's and other times we need to move 3 up
                    // to arrive at a greater value
                    if heap.has_parent() && heap.parent_value() > current {
                        // all nodes except 0th node (root)
                        heap.move_up();
                    }
                }
            } else if heap.has_children() {
                // streak not at 3, just check that we can move down, and do so
                heap.move_left();
            } else {
                // tree bottomed out, need to move back up the tree a bit
                let current = heap.value();
                heap.move_up();
                heap.move_up();

                if current > heap.value() {
                    heap.move_up();
                }
            }

            if !heap.has_parent() {
                // been here enough that we can try resuming the scan to its original
                // speed (no limiting at all)
                atomic_store!(self.remove_limit, true);
            }
            self.set_limit(heap.value() as usize);
        }
    }

    /// adjust the rate of requests per second down (decrease rate)
    pub(super) fn adjust_down(&self) {
        if let Ok(mut heap) = self.heap.try_write() {
            if heap.has_children() {
                heap.move_right();
                self.set_limit(heap.value() as usize);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// PolicyData builds and sets correct values for the inner heap when set_reqs_sec is called
    fn set_reqs_sec_builds_heap_and_sets_initial_value() {
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        assert_eq!(pd.wait_time, 3500);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);
        assert_eq!(pd.heap.read().unwrap().original, 400);
        assert_eq!(pd.heap.read().unwrap().current, 0);
        assert_eq!(pd.heap.read().unwrap().inner[0], 200);
        assert_eq!(pd.heap.read().unwrap().inner[1], 300);
        assert_eq!(pd.heap.read().unwrap().inner[2], 100);
    }

    #[test]
    /// PolicyData setters/getters tests for code coverage / sanity
    fn policy_data_getters_and_setters() {
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_errors(20);
        assert_eq!(pd.errors.load(Ordering::Relaxed), 20);
        pd.set_limit(200);
        assert_eq!(pd.get_limit(), 200);
    }

    #[test]
    /// PolicyData adjust_down sets the limit to the correct value
    fn policy_data_adjust_down_simple() {
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);
        pd.adjust_down();
        assert_eq!(pd.get_limit(), 100);
    }

    #[test]
    /// PolicyData adjust_down sets the limit to the correct value when no child nodes are present
    fn policy_data_adjust_down_no_children() {
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);
        let mut guard = pd.heap.write().unwrap();
        guard.move_to(250);
        guard.set_value(27);
        pd.set_limit(guard.value() as usize);
        drop(guard);

        pd.adjust_down();
        assert_eq!(pd.get_limit(), 27);
    }

    #[test]
    /// PolicyData adjust_up sets the limit to the correct value
    fn policy_data_adjust_up_simple() {
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);
        pd.adjust_up(&0);
        assert_eq!(pd.get_limit(), 300);
    }

    #[test]
    /// PolicyData adjust_up sets the limit to the correct value
    fn policy_data_adjust_up_with_streak_and_2_moves() {
        // original: 400
        // [200, 300, 100, 350, 250, 150, 50, 375, 325, 275, 225, 175, 125, 75, 25, ...]
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);

        // 2 moves
        pd.heap.write().unwrap().move_to(9);
        assert_eq!(pd.heap.read().unwrap().value(), 275);
        pd.adjust_up(&3);
        assert_eq!(pd.heap.read().unwrap().value(), 300);
        assert_eq!(pd.limit.load(Ordering::Relaxed), 300);
        assert!(!pd.remove_limit.load(Ordering::Relaxed));
    }

    #[test]
    /// PolicyData adjust_up sets the limit to the correct value
    fn policy_data_adjust_up_with_streak_and_2_moves_to_arrive_at_root() {
        // original: 400
        // [200, 300, 100, 350, 250, 150, 50, 375, 325, 275, 225, 175, 125, 75, 25, ...]
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);

        pd.heap.write().unwrap().move_to(4);
        assert_eq!(pd.heap.read().unwrap().value(), 250);
        pd.adjust_up(&3);
        assert_eq!(pd.heap.read().unwrap().value(), 200);
        assert_eq!(pd.limit.load(Ordering::Relaxed), 200);
        assert!(pd.remove_limit.load(Ordering::Relaxed));
    }

    #[test]
    /// PolicyData adjust_up sets the limit to the correct value
    fn policy_data_adjust_up_with_streak_and_2_moves_to_find_less_than_current() {
        // original: 400
        // [200, 300, 100, 350, 250, 150, 50, 375, 325, 275, 225, 175, 125, 75, 25, ...]
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);

        pd.heap.write().unwrap().move_to(15);
        assert_eq!(pd.heap.read().unwrap().value(), 387);
        pd.adjust_up(&3);
        assert_eq!(pd.heap.read().unwrap().value(), 350);
        assert_eq!(pd.limit.load(Ordering::Relaxed), 350);
        assert!(!pd.remove_limit.load(Ordering::Relaxed));
    }

    #[test]
    /// PolicyData adjust_up sets the limit to the correct value
    fn policy_data_adjust_up_with_streak_and_3_moves() {
        // original: 400
        // [200, 300, 100, 350, 250, 150, 50, 375, 325, 275, 225, 175, 125, 75, 25, ...]
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);

        pd.heap.write().unwrap().move_to(19);
        assert_eq!(pd.heap.read().unwrap().value(), 287);
        pd.adjust_up(&3);
        assert_eq!(pd.heap.read().unwrap().value(), 300);
        assert_eq!(pd.limit.load(Ordering::Relaxed), 300);
        assert!(!pd.remove_limit.load(Ordering::Relaxed));
    }

    #[test]
    /// PolicyData adjust_up sets the limit to the correct value
    fn policy_data_adjust_up_with_no_children_2_moves() {
        // original: 400
        // [200, 300, 100, 350, 250, 150, 50, 375, 325, 275, 225, 175, 125, 75, 25, ...]
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);

        pd.heap.write().unwrap().move_to(241);

        assert_eq!(pd.heap.read().unwrap().value(), 41);
        pd.adjust_up(&0);
        assert_eq!(pd.heap.read().unwrap().value(), 43);
        assert_eq!(pd.limit.load(Ordering::Relaxed), 43);
        assert!(!pd.remove_limit.load(Ordering::Relaxed));
    }

    #[test]
    /// PolicyData adjust_up sets the limit to the correct value
    fn policy_data_adjust_up_with_no_children_3_moves() {
        // original: 400
        // [200, 300, 100, 350, 250, 150, 50, 375, 325, 275, 225, 175, 125, 75, 25, ...]
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);
        assert_eq!(pd.get_limit(), 200);

        pd.heap.write().unwrap().move_to(240);

        assert_eq!(pd.heap.read().unwrap().value(), 45);
        pd.adjust_up(&0);
        assert_eq!(pd.heap.read().unwrap().value(), 37);
        assert_eq!(pd.limit.load(Ordering::Relaxed), 37);
        assert!(!pd.remove_limit.load(Ordering::Relaxed));
    }

    #[test]
    /// hit some of the out of the way corners of limitheap for coverage
    fn increase_limit_heap_coverage_by_hitting_edge_cases() {
        let pd = PolicyData::new(RequesterPolicy::AutoBail, 7);
        pd.set_reqs_sec(400);

        println!("{:?}", pd.heap.read().unwrap()); // debug derivation

        pd.heap.write().unwrap().move_to(240);
        assert_eq!(pd.heap.write().unwrap().move_right(), 240);
        assert_eq!(pd.heap.write().unwrap().move_left(), 240);

        pd.heap.write().unwrap().move_to(0);
        assert_eq!(pd.heap.write().unwrap().move_up(), 0);
        assert_eq!(pd.heap.write().unwrap().parent_value(), 400);
    }
}
