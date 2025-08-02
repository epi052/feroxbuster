# Integration Tests for Feroxbuster

This directory contains integration tests for feroxbuster using real HTTP servers instead of mocks.

## Auto-Bail Integration Tests

The auto-bail functionality is tested against real servers to validate timeout and error handling behavior.

### test_integration_caddy.rs

Contains two integration tests for auto-bail with timeouts:

#### 1. Python Server Test (`integration_auto_bail_cancels_scan_with_timeouts`)

- **Purpose**: Tests auto-bail behavior with real timeout conditions
- **Server**: Python HTTP server with 5-second delays
- **Requirements**: Python 3 (usually pre-installed)
- **Run**: `cargo test integration_auto_bail_cancels_scan_with_timeouts --test test_integration_caddy -- --exact --ignored --nocapture`

#### 2. Caddy Server Test (`integration_auto_bail_with_caddy`)

- **Purpose**: Tests auto-bail behavior using Caddy web server
- **Server**: Caddy with connection termination for timeout paths
- **Requirements**: Caddy web server
- **Install Caddy**: 
  ```bash
  sudo snap install caddy
  # or
  sudo apt install caddy
  ```
- **Run**: `cargo test integration_auto_bail_with_caddy --test test_integration_caddy -- --exact --ignored --nocapture`

## Test Structure

Both tests follow the same pattern:

1. Start a real HTTP server on a random port
2. Configure server to delay/terminate connections for `/timeout*` paths  
3. Create a wordlist with timeout-triggering and normal words
4. Run feroxbuster with auto-bail enabled
5. Analyze debug logs for timeout errors and auto-bail behavior
6. Clean up server and temporary files

## Why Integration Tests?

While mock server tests provide controlled scenarios, integration tests offer:

- Real network stack behavior
- Actual timeout and connection handling
- Validation against real server implementations  
- Detection of edge cases not covered by mocks

## Running All Integration Tests

```bash
# Run only Python-based test (no external deps needed)
cargo test integration_auto_bail_cancels_scan_with_timeouts --test test_integration_caddy -- --exact --ignored

# Run Caddy test (requires Caddy installation)
cargo test integration_auto_bail_with_caddy --test test_integration_caddy -- --exact --ignored

# Run all integration tests
cargo test --test test_integration_caddy -- --ignored
```

## Expected Behavior

The integration tests validate that:

- Feroxbuster correctly generates timeout errors against slow servers
- Auto-bail logic processes these errors appropriately  
- The scan completes successfully (auto-bail doesn't cause crashes)
- Debug logs contain proper error reporting and statistics

Note: Auto-bail timing may differ between mock and integration tests due to real network conditions.
