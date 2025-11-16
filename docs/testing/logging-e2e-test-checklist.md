# Tracing Logging System - End-to-End Test Checklist

This document provides a comprehensive checklist for manually testing the tracing logging system implementation.

## Prerequisites

- Server and Agent binaries built in release mode
- `jq` installed for JSON parsing
- `curl` installed for API testing

## Test Environment Setup

```bash
# Create test directories
mkdir -p /tmp/opsbox-test-logs
mkdir -p /tmp/opsbox-agent-test-logs
mkdir -p /tmp/opsbox-test-db

# Build binaries
cargo build --release --bin opsbox-server
cargo build --release --bin agent
```

## Test 1: Server Startup and Log Initialization

### Requirements Tested
- 1.1: Server uses tracing
- 2.1: Daily log rotation
- 6.1, 6.3: Custom log directory
- 8.1: Console and file output

### Test Steps

1. **Start Server with custom log directory:**
   ```bash
   ./target/release/opsbox-server \
       --log-dir /tmp/opsbox-test-logs \
       --log-retention 7 \
       --db-path /tmp/opsbox-test-db/server.db \
       --listen-port 14000
   ```

2. **Verify log file creation:**
   ```bash
   ls -la /tmp/opsbox-test-logs/
   # Should see: opsbox-server.log
   ```

3. **Verify log content:**
   ```bash
   cat /tmp/opsbox-test-logs/opsbox-server.log
   # Should contain:
   # - ISO 8601 timestamps (2024-01-15T10:30:45.123Z)
   # - Log levels (INFO, DEBUG, etc.)
   # - Module paths
   # - Startup messages
   ```

4. **Verify console output:**
   - Check terminal for colored log output
   - Verify same messages appear in both console and file

### Expected Results
- ✅ Log file created at specified path
- ✅ Log contains ISO 8601 timestamps
- ✅ Log contains log levels
- ✅ Log contains startup messages
- ✅ Console shows colored output
- ✅ Server API responds on port 14000

---

## Test 2: Agent Startup and Log Initialization

### Requirements Tested
- 1.2: Agent uses tracing
- 2.2: Daily log rotation
- 6.2, 6.4: Custom log directory
- 8.2: Console and file output

### Test Steps

1. **Start Agent with custom log directory:**
   ```bash
   ./target/release/agent \
       --log-dir /tmp/opsbox-agent-test-logs \
       --log-retention 7 \
       --listen-port 14001
   ```

2. **Verify log file creation:**
   ```bash
   ls -la /tmp/opsbox-agent-test-logs/
   # Should see: opsbox-agent.log
   ```

3. **Verify log content:**
   ```bash
   cat /tmp/opsbox-agent-test-logs/opsbox-agent.log
   # Should contain startup messages and proper formatting
   ```

### Expected Results
- ✅ Log file created at specified path
- ✅ Log contains proper timestamps and levels
- ✅ Agent API responds on port 14001

---

## Test 3: Dynamic Log Level Changes

### Requirements Tested
- 5.1, 5.2, 5.3: Dynamic log level API
- 3.4: Configuration persistence

### Test Steps

1. **Get current Server log level:**
   ```bash
   curl -s http://localhost:14000/api/v1/log/config | jq
   ```
   Expected output:
   ```json
   {
     "level": "info",
     "retention_count": 7,
     "log_dir": "/tmp/opsbox-test-logs"
   }
   ```

2. **Change Server log level to DEBUG:**
   ```bash
   curl -X PUT http://localhost:14000/api/v1/log/level \
       -H "Content-Type: application/json" \
       -d '{"level":"debug"}' | jq
   ```
   Expected output:
   ```json
   {
     "message": "Log level updated successfully"
   }
   ```

3. **Verify the change:**
   ```bash
   curl -s http://localhost:14000/api/v1/log/config | jq '.level'
   # Should return: "debug"
   ```

4. **Verify DEBUG logs appear:**
   ```bash
   # Make some API calls to generate logs
   curl -s http://localhost:14000/api/v1/log/config > /dev/null
   
   # Check for DEBUG level logs
   tail -20 /tmp/opsbox-test-logs/opsbox-server.log | grep DEBUG
   # Should see DEBUG level log entries
   ```

5. **Test Agent log level change:**
   ```bash
   # Change to TRACE
   curl -X PUT http://localhost:14001/api/v1/log/level \
       -H "Content-Type: application/json" \
       -d '{"level":"trace"}' | jq
   
   # Verify
   curl -s http://localhost:14001/api/v1/log/config | jq '.level'
   # Should return: "trace"
   ```

6. **Test all log levels:**
   ```bash
   for level in error warn info debug trace; do
       echo "Testing level: $level"
       curl -X PUT http://localhost:14000/api/v1/log/level \
           -H "Content-Type: application/json" \
           -d "{\"level\":\"$level\"}" | jq
       sleep 1
   done
   ```

### Expected Results
- ✅ Log level changes immediately without restart
- ✅ New log level persisted to database
- ✅ Logs at new level appear in log file
- ✅ All log levels (ERROR, WARN, INFO, DEBUG, TRACE) work correctly

---

## Test 4: Log Retention Configuration

### Requirements Tested
- 3.1, 3.2, 3.3: Dynamic retention configuration
- 3.5: Configuration persistence

### Test Steps

1. **Update Server log retention:**
   ```bash
   curl -X PUT http://localhost:14000/api/v1/log/retention \
       -H "Content-Type: application/json" \
       -d '{"retention_count":14}' | jq
   ```

2. **Verify the change:**
   ```bash
   curl -s http://localhost:14000/api/v1/log/config | jq '.retention_count'
   # Should return: 14
   ```

3. **Update Agent log retention:**
   ```bash
   curl -X PUT http://localhost:14001/api/v1/log/retention \
       -H "Content-Type: application/json" \
       -d '{"retention_count":30}' | jq
   
   curl -s http://localhost:14001/api/v1/log/config | jq '.retention_count'
   # Should return: 30
   ```

4. **Restart services and verify persistence:**
   ```bash
   # Stop and restart server
   # Check that retention_count is still 14
   curl -s http://localhost:14000/api/v1/log/config | jq '.retention_count'
   ```

### Expected Results
- ✅ Retention count updates successfully
- ✅ Configuration persisted to database
- ✅ Configuration survives service restart

---

## Test 5: Log File Rolling

### Requirements Tested
- 2.1, 2.2: Daily log rotation
- 2.3: File size limits
- 2.4: Date timestamp in filename

### Test Steps

1. **Check current log files:**
   ```bash
   ls -lh /tmp/opsbox-test-logs/
   ```

2. **Simulate log rolling (manual test):**
   - Note: Automatic rolling happens daily
   - To test manually, you can:
     - Change system date (requires sudo)
     - Or wait for next day
     - Or generate large volume of logs

3. **Generate large volume of logs:**
   ```bash
   # Set to DEBUG level for more logs
   curl -X PUT http://localhost:14000/api/v1/log/level \
       -H "Content-Type: application/json" \
       -d '{"level":"debug"}'
   
   # Make many API calls
   for i in {1..1000}; do
       curl -s http://localhost:14000/api/v1/log/config > /dev/null
   done
   
   # Check log file size
   ls -lh /tmp/opsbox-test-logs/opsbox-server.log
   ```

4. **Verify log file naming:**
   ```bash
   # After rolling, should see files like:
   # opsbox-server.2024-01-15.log
   # opsbox-server.2024-01-16.log
   # opsbox-server.log (current)
   ```

### Expected Results
- ✅ Log files roll daily
- ✅ Old log files have date in filename
- ✅ Current log file is always `opsbox-server.log`
- ✅ Old logs are retained according to retention_count

---

## Test 6: Log Retention Policy

### Requirements Tested
- 3.1, 3.2, 3.3: Retention policy enforcement
- 9.5: Automatic cleanup

### Test Steps

1. **Set low retention count:**
   ```bash
   curl -X PUT http://localhost:14000/api/v1/log/retention \
       -H "Content-Type: application/json" \
       -d '{"retention_count":2}'
   ```

2. **Create multiple old log files (simulation):**
   ```bash
   # Manually create dated log files for testing
   touch /tmp/opsbox-test-logs/opsbox-server.2024-01-10.log
   touch /tmp/opsbox-test-logs/opsbox-server.2024-01-11.log
   touch /tmp/opsbox-test-logs/opsbox-server.2024-01-12.log
   touch /tmp/opsbox-test-logs/opsbox-server.2024-01-13.log
   touch /tmp/opsbox-test-logs/opsbox-server.2024-01-14.log
   
   ls -la /tmp/opsbox-test-logs/
   ```

3. **Trigger log rolling:**
   - Wait for next day or simulate date change
   - Old files beyond retention count should be deleted

4. **Verify cleanup:**
   ```bash
   ls -la /tmp/opsbox-test-logs/
   # Should only see retention_count + 1 files (current + old)
   ```

### Expected Results
- ✅ Old log files beyond retention count are deleted
- ✅ Most recent files are kept
- ✅ Cleanup happens automatically during rolling

---

## Test 7: Log Format and Content

### Requirements Tested
- 8.1, 8.2, 8.3, 8.4, 8.5: Log format requirements
- 8.6, 8.7: Console vs file output

### Test Steps

1. **Verify timestamp format:**
   ```bash
   grep -E "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}" \
       /tmp/opsbox-test-logs/opsbox-server.log
   # Should match ISO 8601 format
   ```

2. **Verify log levels present:**
   ```bash
   grep -E "(ERROR|WARN|INFO|DEBUG|TRACE)" \
       /tmp/opsbox-test-logs/opsbox-server.log
   ```

3. **Verify module paths:**
   ```bash
   # Logs should include module information
   cat /tmp/opsbox-test-logs/opsbox-server.log | head -20
   ```

4. **Verify structured fields:**
   ```bash
   # Check for key-value pairs in logs
   grep -E "\w+=\w+" /tmp/opsbox-test-logs/opsbox-server.log
   ```

5. **Compare console vs file output:**
   - Console should have colors (ANSI codes)
   - File should be plain text
   - Content should be identical otherwise

### Expected Results
- ✅ Timestamps in ISO 8601 format
- ✅ Log levels clearly visible
- ✅ Module paths included
- ✅ Structured fields supported
- ✅ Console has colors, file is plain text

---

## Test 8: Parameter Validation

### Requirements Tested
- 5.4: Input validation
- Error handling

### Test Steps

1. **Test invalid log level:**
   ```bash
   curl -X PUT http://localhost:14000/api/v1/log/level \
       -H "Content-Type: application/json" \
       -d '{"level":"invalid"}' -w "\nHTTP Code: %{http_code}\n"
   # Should return 400 or 422
   ```

2. **Test invalid retention count:**
   ```bash
   curl -X PUT http://localhost:14000/api/v1/log/retention \
       -H "Content-Type: application/json" \
       -d '{"retention_count":-1}' -w "\nHTTP Code: %{http_code}\n"
   # Should return 400 or 422
   ```

3. **Test missing parameters:**
   ```bash
   curl -X PUT http://localhost:14000/api/v1/log/level \
       -H "Content-Type: application/json" \
       -d '{}' -w "\nHTTP Code: %{http_code}\n"
   # Should return 400 or 422
   ```

4. **Test malformed JSON:**
   ```bash
   curl -X PUT http://localhost:14000/api/v1/log/level \
       -H "Content-Type: application/json" \
       -d '{invalid json}' -w "\nHTTP Code: %{http_code}\n"
   # Should return 400
   ```

### Expected Results
- ✅ Invalid log levels rejected with 400/422
- ✅ Invalid retention counts rejected with 400/422
- ✅ Missing parameters rejected with 400/422
- ✅ Malformed JSON rejected with 400
- ✅ Error messages are clear and helpful

---

## Test 9: Performance Under Load

### Requirements Tested
- 9.1, 9.2, 9.3: Performance requirements
- Async logging

### Test Steps

1. **Generate high volume of logs:**
   ```bash
   # Set to DEBUG for more logs
   curl -X PUT http://localhost:14000/api/v1/log/level \
       -H "Content-Type: application/json" \
       -d '{"level":"debug"}'
   
   # Make many concurrent requests
   for i in {1..100}; do
       curl -s http://localhost:14000/api/v1/log/config > /dev/null &
   done
   wait
   ```

2. **Monitor resource usage:**
   ```bash
   # In another terminal
   top -pid $(pgrep opsbox-server)
   # Monitor CPU and memory usage
   ```

3. **Check log file size:**
   ```bash
   ls -lh /tmp/opsbox-test-logs/opsbox-server.log
   ```

4. **Verify no log loss:**
   ```bash
   # Count log entries
   wc -l /tmp/opsbox-test-logs/opsbox-server.log
   ```

### Expected Results
- ✅ Server remains responsive under load
- ✅ CPU usage remains reasonable (<50%)
- ✅ Memory usage stable (no leaks)
- ✅ No log entries lost
- ✅ Log writes don't block main thread

---

## Test 10: Agent Manager Proxy (if applicable)

### Requirements Tested
- 5.1, 5.2, 5.3: Agent log config via proxy

### Test Steps

1. **Register Agent with Server:**
   ```bash
   # Assuming agent is registered with ID "test-agent"
   # and has tags: host=localhost, listen_port=14001
   ```

2. **Get Agent log config via proxy:**
   ```bash
   curl -s http://localhost:14000/api/v1/agents/test-agent/log/config | jq
   ```

3. **Update Agent log level via proxy:**
   ```bash
   curl -X PUT http://localhost:14000/api/v1/agents/test-agent/log/level \
       -H "Content-Type: application/json" \
       -d '{"level":"debug"}' | jq
   ```

4. **Verify change on Agent:**
   ```bash
   curl -s http://localhost:14001/api/v1/log/config | jq '.level'
   # Should return: "debug"
   ```

5. **Test offline Agent:**
   ```bash
   # Stop agent
   # Try to update config
   curl -X PUT http://localhost:14000/api/v1/agents/test-agent/log/level \
       -H "Content-Type: application/json" \
       -d '{"level":"info"}' -w "\nHTTP Code: %{http_code}\n"
   # Should return 502 Bad Gateway
   ```

### Expected Results
- ✅ Proxy successfully forwards requests to Agent
- ✅ Configuration changes work through proxy
- ✅ Offline Agent returns 502 error
- ✅ Non-existent Agent returns 404 error

---

## Test 11: Service Restart Persistence

### Requirements Tested
- 3.4, 3.5: Configuration persistence

### Test Steps

1. **Set custom configuration:**
   ```bash
   curl -X PUT http://localhost:14000/api/v1/log/level \
       -H "Content-Type: application/json" \
       -d '{"level":"debug"}'
   
   curl -X PUT http://localhost:14000/api/v1/log/retention \
       -H "Content-Type: application/json" \
       -d '{"retention_count":21}'
   ```

2. **Verify configuration:**
   ```bash
   curl -s http://localhost:14000/api/v1/log/config | jq
   ```

3. **Restart Server:**
   ```bash
   # Stop server (Ctrl+C)
   # Start server again with same parameters
   ./target/release/opsbox-server \
       --log-dir /tmp/opsbox-test-logs \
       --db-path /tmp/opsbox-test-db/server.db \
       --listen-port 14000
   ```

4. **Verify configuration persisted:**
   ```bash
   curl -s http://localhost:14000/api/v1/log/config | jq
   # Should still show level=debug, retention_count=21
   ```

### Expected Results
- ✅ Configuration saved to database
- ✅ Configuration loaded on startup
- ✅ Log level applied immediately
- ✅ Retention count preserved

---

## Test Summary Checklist

### Core Functionality
- [ ] Server logging initialization
- [ ] Agent logging initialization
- [ ] Log file creation
- [ ] Console output
- [ ] File output
- [ ] Log format (timestamps, levels, modules)

### Dynamic Configuration
- [ ] Get log configuration API
- [ ] Update log level API (Server)
- [ ] Update log level API (Agent)
- [ ] Update retention API (Server)
- [ ] Update retention API (Agent)
- [ ] Configuration persistence
- [ ] Configuration survives restart

### Log Management
- [ ] Daily log rotation
- [ ] Log file naming with dates
- [ ] Retention policy enforcement
- [ ] Old log cleanup
- [ ] Log directory creation

### Error Handling
- [ ] Invalid log level rejected
- [ ] Invalid retention count rejected
- [ ] Missing parameters rejected
- [ ] Malformed JSON rejected
- [ ] Clear error messages

### Performance
- [ ] Async logging (non-blocking)
- [ ] Reasonable CPU usage
- [ ] Stable memory usage
- [ ] No log loss under load
- [ ] Service remains responsive

### Integration (if applicable)
- [ ] Agent Manager proxy works
- [ ] Offline Agent handling
- [ ] Non-existent Agent handling

---

## Cleanup

```bash
# Stop services
pkill opsbox-server
pkill agent

# Remove test directories
rm -rf /tmp/opsbox-test-logs
rm -rf /tmp/opsbox-agent-test-logs
rm -rf /tmp/opsbox-test-db
```

---

## Notes

- Some tests (like log rolling) require waiting for time-based events
- Performance tests should be run on a representative system
- Frontend tests should be performed separately using the web interface
- Integration tests with Agent Manager require proper agent registration

## References

- Requirements: `.kiro/specs/tracing-logging-system/requirements.md`
- Design: `.kiro/specs/tracing-logging-system/design.md`
- Tasks: `.kiro/specs/tracing-logging-system/tasks.md`
