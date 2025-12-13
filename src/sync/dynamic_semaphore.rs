use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{Semaphore, SemaphorePermit};

/// A wrapper around Tokio's [`Semaphore`] that supports dynamic capacity reduction.
///
/// Unlike the standard Tokio semaphore, this implementation allows for reduction of the
/// effective capacity even when permits are already acquired and other tasks are waiting.
/// This is particularly useful for rate limiting scenarios where we need to dynamically
/// adjust the concurrency level based on runtime conditions.
///
/// # Key Features
///
/// - **Dynamic Capacity Reduction**: Can reduce capacity even when permits are in use
/// - **Queued Waiter Preservation**: Existing waiters remain in queue during capacity changes
/// - **Thread-Safe**: All operations are atomic and safe for concurrent use
/// - **Drop Safety**: Automatically manages capacity when permits are released
///
/// # Example
///
/// ```rust,no_run
/// use feroxbuster::sync::DynamicSemaphore;
///
/// #[tokio::main]
/// async fn main() {
///     let semaphore = DynamicSemaphore::new(2);
///     
///     // Acquire permits
///     let _permit1 = semaphore.acquire().await.unwrap();
///     let _permit2 = semaphore.acquire().await.unwrap();
///     
///     // Reduce capacity from 2 to 1 (takes effect when permits are released)
///     semaphore.reduce_capacity(1);
///     
///     // When permits are dropped, only 1 permit will be available instead of 2
/// }
/// ```

#[derive(Debug)]
pub struct DynamicSemaphore {
    /// The underlying Tokio semaphore that handles the actual permit management
    inner: Arc<Semaphore>,

    /// The current maximum capacity for this semaphore
    ///
    /// This value represents the desired maximum number of permits that should be
    /// available. When permits are released, the semaphore ensures that the total
    /// available permits never exceed this capacity.
    max_capacity: AtomicUsize,

    /// Counter for permits currently in use
    ///
    /// This is incremented when permits are acquired and decremented when released.
    /// We use this to track how many permits are actually in use vs the virtual capacity.
    permits_in_use: AtomicUsize,
}

/// A permit acquired from a [`DynamicSemaphore`].
///
/// This permit automatically manages the dynamic capacity when dropped. If releasing
/// the permit would cause the semaphore to exceed its current capacity limit, the
/// permit is "forgotten" instead of being returned to the available pool.
///
/// The permit provides the same guarantees as Tokio's [`SemaphorePermit`] but with
/// additional capacity management logic.
#[derive(Debug)]
pub struct DynamicSemaphorePermit<'a> {
    /// The underlying Tokio semaphore permit
    ///
    /// This is wrapped in an Option to allow for controlled dropping during
    /// capacity management in the Drop implementation.
    permit: Option<SemaphorePermit<'a>>,

    /// Reference to the parent semaphore for capacity checking
    semaphore: &'a DynamicSemaphore,
}

impl DynamicSemaphore {
    /// Creates a new [`DynamicSemaphore`] with the specified number of permits.
    ///
    /// # Arguments
    ///
    /// * `permits` - The initial number of permits available in the semaphore
    ///
    /// # Panics
    ///
    /// Panics if `permits` exceeds the maximum number of permits supported by
    /// the underlying Tokio semaphore implementation.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    ///
    /// let semaphore = DynamicSemaphore::new(10);
    /// assert_eq!(semaphore.current_capacity(), 10);
    /// ```
    pub fn new(permits: usize) -> Self {
        Self {
            inner: Arc::new(Semaphore::new(permits)),
            max_capacity: AtomicUsize::new(permits),
            permits_in_use: AtomicUsize::new(0),
        }
    }

    /// Acquires a permit from the semaphore.
    ///
    /// This method will wait until a permit becomes available. The returned permit
    /// will automatically manage capacity constraints when dropped.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing a [`DynamicSemaphorePermit`] on success, or an
    /// [`tokio::sync::AcquireError`] if the semaphore has been closed.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = DynamicSemaphore::new(1);
    ///     let permit = semaphore.acquire().await.unwrap();
    ///     // permit is automatically released when dropped
    /// }
    /// ```
    pub async fn acquire(&self) -> Result<DynamicSemaphorePermit<'_>, tokio::sync::AcquireError> {
        loop {
            // Check if we're already at or over capacity before acquiring
            let current_in_use = self.permits_in_use.load(Ordering::Acquire);
            let current_capacity = self.current_capacity();

            if current_in_use >= current_capacity {
                // We're at or over capacity, wait for a permit to be released
                let _temp_permit = self.inner.acquire().await?;
                // Drop the permit immediately and try again - this ensures we wait
                // for permits to become available but don't actually consume them
                // if we're over capacity
                drop(_temp_permit);
                continue;
            }

            // Try to acquire a permit
            let permit = self.inner.acquire().await?;

            // Atomically increment in_use and check if we're still within capacity
            let new_in_use = self.permits_in_use.fetch_add(1, Ordering::AcqRel) + 1;

            if new_in_use <= current_capacity {
                // We're within capacity, return the permit
                return Ok(DynamicSemaphorePermit {
                    permit: Some(permit),
                    semaphore: self,
                });
            } else {
                // We exceeded capacity between the check and increment, backtrack
                self.permits_in_use.fetch_sub(1, Ordering::AcqRel);
                drop(permit);
                // implicit try again
            }
        }
    }

    /// Attempts to acquire a permit without waiting.
    ///
    /// If a permit is immediately available, it is returned. Otherwise, this method
    /// returns an error indicating why the permit could not be acquired.
    ///
    /// # Returns
    ///
    /// A [`Result`] containing a [`DynamicSemaphorePermit`] if successful, or a
    /// [`tokio::sync::TryAcquireError`] if no permit is available or the semaphore is closed.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    /// use tokio::sync::TryAcquireError;
    ///
    /// let semaphore = DynamicSemaphore::new(1);
    /// match semaphore.try_acquire() {
    ///     Ok(permit) => println!("Got permit"),
    ///     Err(TryAcquireError::NoPermits) => println!("No permits available"),
    ///     Err(TryAcquireError::Closed) => println!("Semaphore closed"),
    /// };
    /// ```
    pub fn try_acquire(&self) -> Result<DynamicSemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        // Check if we're already at or over capacity
        let current_in_use = self.permits_in_use.load(Ordering::Acquire);
        let current_capacity = self.current_capacity();

        if current_in_use >= current_capacity {
            // We're at or over capacity, cannot acquire
            return Err(tokio::sync::TryAcquireError::NoPermits);
        }

        // Try to acquire a permit from the underlying semaphore
        let permit = self.inner.try_acquire()?;

        // Atomically increment in_use and check if we're still within capacity
        let new_in_use = self.permits_in_use.fetch_add(1, Ordering::AcqRel) + 1;
        if new_in_use <= current_capacity {
            // We're within capacity, return the permit
            Ok(DynamicSemaphorePermit {
                permit: Some(permit),
                semaphore: self,
            })
        } else {
            // We exceeded capacity between the check and increment, backtrack
            self.permits_in_use.fetch_sub(1, Ordering::AcqRel);
            drop(permit);
            Err(tokio::sync::TryAcquireError::NoPermits)
        }
    }

    /// Reduces the maximum capacity of the semaphore.
    ///
    /// This method sets a new maximum capacity for the semaphore. The change takes
    /// effect immediately for new permit acquisitions. If there are currently more
    /// permits in use than the new capacity allows, the reduction will take effect
    /// gradually as permits are released.
    ///
    /// # Arguments
    ///
    /// * `new_capacity` - The new maximum number of permits that should be available
    ///
    /// # Returns
    ///
    /// The previous capacity value before the change.
    ///
    /// # Notes
    ///
    /// - This operation is atomic and thread-safe
    /// - Existing permit holders are not affected until they release their permits
    /// - Queued waiters remain in the queue and will eventually be served
    /// - If available permits exceed the new capacity, excess permits are immediately forgotten
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = DynamicSemaphore::new(5);
    ///     
    ///     // Reduce capacity from 5 to 2
    ///     let old_capacity = semaphore.reduce_capacity(2);
    ///     assert_eq!(old_capacity, 5);
    ///     assert_eq!(semaphore.current_capacity(), 2);
    /// }
    /// ```
    pub fn reduce_capacity(&self, new_capacity: usize) -> usize {
        let old_capacity = self.max_capacity.swap(new_capacity, Ordering::AcqRel);

        // If we're reducing capacity and there are available permits that exceed
        // the new capacity, we should forget the excess permits immediately
        if new_capacity < old_capacity {
            let available = self.inner.available_permits();
            let to_forget = available.saturating_sub(new_capacity);

            if to_forget > 0 {
                self.inner.forget_permits(to_forget);
            }
        }

        old_capacity
    }

    /// Increases the maximum capacity of the semaphore.
    ///
    /// This method sets a new maximum capacity that is higher than the current one.
    /// Additional permits are immediately added to the semaphore up to the new capacity.
    ///
    /// # Arguments
    ///
    /// * `new_capacity` - The new maximum number of permits that should be available
    ///
    /// # Returns
    ///
    /// The previous capacity value before the change.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity would cause the semaphore to exceed its maximum
    /// supported permit count.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = DynamicSemaphore::new(2);
    ///     
    ///     // Increase capacity from 2 to 5
    ///     let old_capacity = semaphore.increase_capacity(5);
    ///     assert_eq!(old_capacity, 2);
    ///     assert_eq!(semaphore.current_capacity(), 5);
    /// }
    /// ```
    pub fn increase_capacity(&self, new_capacity: usize) -> usize {
        let old_capacity = self.max_capacity.swap(new_capacity, Ordering::AcqRel);

        // If we're increasing capacity, add the additional permits
        if new_capacity > old_capacity {
            let to_add = new_capacity - old_capacity;
            self.inner.add_permits(to_add);
        }

        old_capacity
    }

    /// Returns the current maximum capacity of the semaphore.
    ///
    /// This represents the maximum number of permits that can be available at any
    /// given time, which may be different from the number of currently available permits.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    ///
    /// let semaphore = DynamicSemaphore::new(10);
    /// assert_eq!(semaphore.current_capacity(), 10);
    /// ```
    pub fn current_capacity(&self) -> usize {
        self.max_capacity.load(Ordering::Acquire)
    }

    /// Returns the number of permits currently available for immediate acquisition.
    ///
    /// This value represents permits that can be acquired without waiting. Note that
    /// this number may be less than the capacity if permits are currently in use.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = DynamicSemaphore::new(3);
    ///     assert_eq!(semaphore.available_permits(), 3);
    ///     
    ///     let _permit = semaphore.acquire().await.unwrap();
    ///     assert_eq!(semaphore.available_permits(), 2);
    /// }
    /// ```
    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    /// Closes the semaphore, preventing new permits from being acquired.
    ///
    /// This will wake up all tasks currently waiting to acquire a permit, causing
    /// them to receive an [`tokio::sync::AcquireError`]. Existing permits remain
    /// valid until dropped.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = DynamicSemaphore::new(1);
    ///     semaphore.close();
    ///     
    ///     // This will return an error
    ///     assert!(semaphore.acquire().await.is_err());
    /// }
    /// ```
    pub fn close(&self) {
        self.inner.close();
    }

    /// Returns whether the semaphore has been closed.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    ///
    /// let semaphore = DynamicSemaphore::new(1);
    /// assert!(!semaphore.is_closed());
    ///
    /// semaphore.close();
    /// assert!(semaphore.is_closed());
    /// ```
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Returns the current number of permits in use (for debugging).
    ///
    /// This is primarily useful for debugging and testing to understand
    /// the internal state of the semaphore.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use feroxbuster::sync::DynamicSemaphore;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let semaphore = DynamicSemaphore::new(3);
    ///     assert_eq!(semaphore.permits_in_use(), 0);
    ///
    ///     let _permit = semaphore.acquire().await.unwrap();
    ///     assert_eq!(semaphore.permits_in_use(), 1);
    /// }
    /// ```
    pub fn permits_in_use(&self) -> usize {
        self.permits_in_use.load(Ordering::Acquire)
    }
}

impl<'a> Drop for DynamicSemaphorePermit<'a> {
    /// Handles the automatic release of the permit with capacity management.
    ///
    /// This implementation uses an approach designed to avoid race conditions:
    ///
    /// We make the decision atomically BEFORE releasing the permit by checking if we're
    /// currently over capacity. If we are, we "forget" the permit instead of releasing it.
    /// If we're not over capacity, we release it normally.
    ///
    /// This works because:
    /// 1. We decrement permits_in_use first (atomically)
    /// 2. We check if permits_in_use + available_permits > capacity  
    /// 3. If so, we're over capacity and should forget this permit
    /// 4. If not, we can safely release it
    ///
    /// The key insight is that permits_in_use represents permits about to be released,
    /// so permits_in_use + available_permits tells us what the total would be after release.
    fn drop(&mut self) {
        if let Some(permit) = self.permit.take() {
            // First, atomically decrement our usage counter
            self.semaphore.permits_in_use.fetch_sub(1, Ordering::AcqRel);

            // Check current state
            let current_capacity = self.semaphore.current_capacity();
            let current_available = self.semaphore.available_permits();

            // Calculate what the total would be if we released this permit
            let total_after_release = current_available + 1;

            // If releasing would exceed capacity, forget the permit instead
            if total_after_release > current_capacity {
                // Forget the permit - it never gets added to available permits
                permit.forget();
            } else {
                // Safe to release normally
                drop(permit);
            }
        }
    }
}

// Ensure the permit can be safely sent between threads
unsafe impl<'a> Send for DynamicSemaphorePermit<'a> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_basic_acquire_release() {
        let semaphore = DynamicSemaphore::new(2);

        assert_eq!(semaphore.available_permits(), 2);
        assert_eq!(semaphore.current_capacity(), 2);
        assert_eq!(semaphore.permits_in_use(), 0);

        let permit1 = semaphore.acquire().await.unwrap();
        assert_eq!(semaphore.available_permits(), 1);
        assert_eq!(semaphore.permits_in_use(), 1);

        let permit2 = semaphore.acquire().await.unwrap();
        assert_eq!(semaphore.available_permits(), 0);
        assert_eq!(semaphore.permits_in_use(), 2);

        drop(permit1);
        assert_eq!(semaphore.available_permits(), 1);
        assert_eq!(semaphore.permits_in_use(), 1);

        drop(permit2);
        assert_eq!(semaphore.available_permits(), 2);
        assert_eq!(semaphore.permits_in_use(), 0);
    }

    #[tokio::test]
    async fn test_capacity_reduction() {
        let semaphore = DynamicSemaphore::new(3);

        // Acquire all permits
        let permit1 = semaphore.acquire().await.unwrap();
        let permit2 = semaphore.acquire().await.unwrap();
        let permit3 = semaphore.acquire().await.unwrap();

        assert_eq!(semaphore.available_permits(), 0);
        assert_eq!(semaphore.permits_in_use(), 3);

        // Reduce capacity to 2
        let old_capacity = semaphore.reduce_capacity(2);
        assert_eq!(old_capacity, 3);
        assert_eq!(semaphore.current_capacity(), 2);

        // Drop one permit - should be returned since we're within the new capacity (0 + 1 <= 2)
        drop(permit1);
        assert_eq!(semaphore.available_permits(), 1);
        assert_eq!(semaphore.permits_in_use(), 2);

        // Drop another permit - should be returned since we're still within capacity (1 + 1 <= 2)
        drop(permit2);
        assert_eq!(semaphore.available_permits(), 2);
        assert_eq!(semaphore.permits_in_use(), 1);

        // Drop the last permit - this would exceed capacity (2 + 1 > 2), so should be forgotten
        drop(permit3);
        assert_eq!(semaphore.available_permits(), 2); // Still 2, excess was forgotten
        assert_eq!(semaphore.permits_in_use(), 0);
    }

    #[tokio::test]
    async fn test_capacity_increase() {
        let semaphore = DynamicSemaphore::new(2);

        assert_eq!(semaphore.available_permits(), 2);

        // Increase capacity
        let old_capacity = semaphore.increase_capacity(5);
        assert_eq!(old_capacity, 2);
        assert_eq!(semaphore.current_capacity(), 5);
        assert_eq!(semaphore.available_permits(), 5);
    }

    #[tokio::test]
    async fn test_try_acquire() {
        let semaphore = DynamicSemaphore::new(1);

        let permit1 = semaphore.try_acquire().unwrap();
        assert!(semaphore.try_acquire().is_err());

        drop(permit1);
        assert!(semaphore.try_acquire().is_ok());
    }

    #[tokio::test]
    async fn test_close() {
        let semaphore = DynamicSemaphore::new(1);

        assert!(!semaphore.is_closed());
        semaphore.close();
        assert!(semaphore.is_closed());

        assert!(semaphore.acquire().await.is_err());
    }

    /// Test that reproduces the exact live site issue that was discovered
    #[tokio::test]
    async fn test_over_capacity_acquisition_prevention() {
        let semaphore = Arc::new(DynamicSemaphore::new(5));

        // Step 1: Acquire permits like a live site would
        let permit1 = semaphore.acquire().await.unwrap();
        let permit2 = semaphore.acquire().await.unwrap();

        assert_eq!(semaphore.available_permits(), 3);
        assert_eq!(semaphore.permits_in_use(), 2);

        // Step 2: Reduce capacity while permits are in use (the critical scenario)
        semaphore.reduce_capacity(1);

        assert_eq!(semaphore.current_capacity(), 1);
        assert_eq!(semaphore.available_permits(), 1); // Should be 1 (5-2=3, but capped at 1)
        assert_eq!(semaphore.permits_in_use(), 2); // Still 2 in use (over capacity)

        // Step 3: Try to acquire a new permit while over capacity - should FAIL
        assert!(
            semaphore.try_acquire().is_err(),
            "Should not be able to acquire when over capacity"
        );

        // Step 4: Release permits and verify capacity is enforced
        drop(permit1);
        assert_eq!(semaphore.available_permits(), 1);
        assert_eq!(semaphore.permits_in_use(), 1);

        drop(permit2);
        assert_eq!(semaphore.available_permits(), 1);
        assert_eq!(semaphore.permits_in_use(), 0);

        // Step 5: Now acquisition should work since we're at capacity
        let permit_new = semaphore.try_acquire().unwrap();
        assert_eq!(semaphore.available_permits(), 0);
        assert_eq!(semaphore.permits_in_use(), 1);

        drop(permit_new);
        assert_eq!(semaphore.available_permits(), 1);
        assert_eq!(semaphore.permits_in_use(), 0);
    }

    /// Test concurrent operations under load to verify race condition fixes
    #[tokio::test]
    async fn test_concurrent_capacity_reduction() {
        let semaphore = Arc::new(DynamicSemaphore::new(10));
        let mut handles = vec![];

        // Start many tasks that acquire permits and hold them briefly
        for _ in 0..20 {
            let sem = semaphore.clone();
            handles.push(tokio::spawn(async move {
                if let Ok(permit) = sem.try_acquire() {
                    sleep(Duration::from_millis(50)).await;
                    drop(permit);
                }
                // Some tasks won't get permits due to capacity limits - this is expected
            }));
        }

        // While tasks are running, reduce capacity
        sleep(Duration::from_millis(10)).await;
        semaphore.reduce_capacity(5);

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify final state - available permits should never exceed capacity
        assert!(semaphore.available_permits() <= semaphore.current_capacity());
        assert_eq!(semaphore.current_capacity(), 5);
    }

    /// Stress test with continuous capacity changes and concurrent acquisitions
    #[tokio::test]
    async fn test_stress_concurrent_operations() {
        let semaphore = Arc::new(DynamicSemaphore::new(50));
        let mut handles = vec![];

        // Start tasks that continuously try to acquire and release permits
        for _ in 0..100 {
            let sem = semaphore.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..5 {
                    if let Ok(permit) = sem.try_acquire() {
                        tokio::task::yield_now().await;
                        drop(permit);
                    }
                    tokio::task::yield_now().await;
                }
            }));
        }

        // Continuously reduce capacity while tasks are running
        let sem_reducer = semaphore.clone();
        let reducer_handle = tokio::spawn(async move {
            for new_capacity in (1..=50).rev() {
                sem_reducer.reduce_capacity(new_capacity);
                tokio::task::yield_now().await;
            }
        });

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }
        reducer_handle.await.unwrap();

        // Final verification - the semaphore should be in a valid state
        assert!(semaphore.available_permits() <= semaphore.current_capacity());
        assert_eq!(semaphore.current_capacity(), 1);
        assert_eq!(semaphore.permits_in_use(), 0);
    }

    /// Test that demonstrates integration scenarios similar to feroxbuster usage
    #[tokio::test]
    async fn test_feroxbuster_integration_scenario() {
        let limiter = Arc::new(DynamicSemaphore::new(3));

        // Simulate 3 active scans by acquiring all permits
        let permit1 = limiter.acquire().await.unwrap();
        let permit2 = limiter.acquire().await.unwrap();
        let permit3 = limiter.acquire().await.unwrap();

        assert_eq!(limiter.available_permits(), 0);
        assert_eq!(limiter.current_capacity(), 3);

        // Simulate user reducing scan limit from 3 to 1 via scan management menu
        limiter.reduce_capacity(1);
        assert_eq!(limiter.current_capacity(), 1);

        // Verify no new scans can start when over capacity
        assert!(limiter.try_acquire().is_err());

        // As scans complete, capacity reduction takes effect
        drop(permit1);
        assert_eq!(limiter.available_permits(), 1);

        drop(permit2);
        assert_eq!(limiter.available_permits(), 1); // Excess forgotten

        drop(permit3);
        assert_eq!(limiter.available_permits(), 1); // Excess forgotten

        // Now only 1 scan can run concurrently
        let _new_permit = limiter.acquire().await.unwrap();
        assert_eq!(limiter.available_permits(), 0);
        assert!(limiter.try_acquire().is_err());
    }

    /// Test edge cases and boundary conditions
    #[tokio::test]
    async fn test_edge_cases() {
        // Test zero capacity
        let semaphore = DynamicSemaphore::new(0);
        assert_eq!(semaphore.current_capacity(), 0);
        assert_eq!(semaphore.available_permits(), 0);
        assert!(semaphore.try_acquire().is_err());

        // Test capacity reduction to zero
        let semaphore = DynamicSemaphore::new(2);
        let permit = semaphore.acquire().await.unwrap();

        semaphore.reduce_capacity(0);
        assert_eq!(semaphore.current_capacity(), 0);
        assert!(semaphore.try_acquire().is_err());

        drop(permit);
        assert_eq!(semaphore.available_permits(), 0);
        assert!(semaphore.try_acquire().is_err());

        // Test large capacity values
        let semaphore = DynamicSemaphore::new(1000);
        assert_eq!(semaphore.current_capacity(), 1000);
        assert_eq!(semaphore.available_permits(), 1000);

        let permit = semaphore.try_acquire().unwrap();
        assert_eq!(semaphore.available_permits(), 999);
        drop(permit);
        assert_eq!(semaphore.available_permits(), 1000);
    }
}
