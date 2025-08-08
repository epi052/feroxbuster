use feroxbuster::sync::DynamicSemaphore;
/// Integration tests for DynamicSemaphore
///
/// These tests verify the complete functionality of the DynamicSemaphore
/// implementation, covering all use cases and edge conditions.
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
