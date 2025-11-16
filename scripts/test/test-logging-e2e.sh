#!/bin/bash
# End-to-End Testing Script for Tracing Logging System
# Tests Server and Agent logging initialization, file rolling, retention, and dynamic level changes

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up test processes...${NC}"
    if [ ! -z "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
    fi
    if [ ! -z "$AGENT_PID" ]; then
        kill $AGENT_PID 2>/dev/null || true
    fi
    
    # Clean up test directories
    rm -rf /tmp/opsbox-test-logs
    rm -rf /tmp/opsbox-agent-test-logs
    rm -rf /tmp/opsbox-test-db
}

trap cleanup EXIT

# Test helper functions
pass_test() {
    echo -e "${GREEN}✓ PASS:${NC} $1"
    ((TESTS_PASSED++))
}

fail_test() {
    echo -e "${RED}✗ FAIL:${NC} $1"
    ((TESTS_FAILED++))
}

info() {
    echo -e "${YELLOW}ℹ INFO:${NC} $1"
}

# Create test directories
setup_test_env() {
    info "Setting up test environment..."
    mkdir -p /tmp/opsbox-test-logs
    mkdir -p /tmp/opsbox-agent-test-logs
    mkdir -p /tmp/opsbox-test-db
}

# Test 1: Server startup and log initialization
test_server_startup() {
    info "Test 1: Server startup and log initialization"
    
    # Build server
    cargo build --release --bin opsbox-server 2>&1 | grep -q "Finished" || {
        fail_test "Server build failed"
        return 1
    }
    
    # Start server with custom log directory
    ./target/release/opsbox-server \
        --log-dir /tmp/opsbox-test-logs \
        --log-retention 3 \
        --db-path /tmp/opsbox-test-db/server.db \
        --listen-port 14000 &
    SERVER_PID=$!
    
    # Wait for server to start
    sleep 3
    
    # Check if server is running
    if ! kill -0 $SERVER_PID 2>/dev/null; then
        fail_test "Server failed to start"
        return 1
    fi
    
    # Check if log file was created
    if [ -f "/tmp/opsbox-test-logs/opsbox-server.log" ]; then
        pass_test "Server log file created"
    else
        fail_test "Server log file not created"
        return 1
    fi
    
    # Check if log contains startup messages
    if grep -q "Starting OpsBox Server" /tmp/opsbox-test-logs/opsbox-server.log 2>/dev/null || \
       grep -q "Server listening" /tmp/opsbox-test-logs/opsbox-server.log 2>/dev/null; then
        pass_test "Server startup logged"
    else
        fail_test "Server startup not logged"
    fi
    
    # Check if server API is responding
    if curl -s http://localhost:14000/api/v1/log/config > /dev/null; then
        pass_test "Server API responding"
    else
        fail_test "Server API not responding"
    fi
}

# Test 2: Agent startup and log initialization
test_agent_startup() {
    info "Test 2: Agent startup and log initialization"
    
    # Build agent
    cargo build --release --bin agent 2>&1 | grep -q "Finished" || {
        fail_test "Agent build failed"
        return 1
    }
    
    # Start agent with custom log directory
    ./target/release/agent \
        --log-dir /tmp/opsbox-agent-test-logs \
        --log-retention 3 \
        --listen-port 14001 &
    AGENT_PID=$!
    
    # Wait for agent to start
    sleep 3
    
    # Check if agent is running
    if ! kill -0 $AGENT_PID 2>/dev/null; then
        fail_test "Agent failed to start"
        return 1
    fi
    
    # Check if log file was created
    if [ -f "/tmp/opsbox-agent-test-logs/opsbox-agent.log" ]; then
        pass_test "Agent log file created"
    else
        fail_test "Agent log file not created"
        return 1
    fi
    
    # Check if log contains startup messages
    if grep -q "Starting" /tmp/opsbox-agent-test-logs/opsbox-agent.log 2>/dev/null || \
       grep -q "Agent listening" /tmp/opsbox-agent-test-logs/opsbox-agent.log 2>/dev/null; then
        pass_test "Agent startup logged"
    else
        fail_test "Agent startup not logged"
    fi
    
    # Check if agent API is responding
    if curl -s http://localhost:14001/api/v1/log/config > /dev/null; then
        pass_test "Agent API responding"
    else
        fail_test "Agent API not responding"
    fi
}

# Test 3: Dynamic log level changes
test_dynamic_log_level() {
    info "Test 3: Dynamic log level changes"
    
    # Get current server log level
    CURRENT_LEVEL=$(curl -s http://localhost:14000/api/v1/log/config | jq -r '.level')
    if [ ! -z "$CURRENT_LEVEL" ]; then
        pass_test "Retrieved current log level: $CURRENT_LEVEL"
    else
        fail_test "Failed to retrieve current log level"
        return 1
    fi
    
    # Change to DEBUG level
    RESPONSE=$(curl -s -X PUT http://localhost:14000/api/v1/log/level \
        -H "Content-Type: application/json" \
        -d '{"level":"debug"}')
    
    if echo "$RESPONSE" | jq -e '.message' > /dev/null 2>&1; then
        pass_test "Changed server log level to DEBUG"
    else
        fail_test "Failed to change server log level"
        return 1
    fi
    
    # Verify the change
    sleep 1
    NEW_LEVEL=$(curl -s http://localhost:14000/api/v1/log/config | jq -r '.level')
    if [ "$NEW_LEVEL" = "debug" ]; then
        pass_test "Server log level change verified"
    else
        fail_test "Server log level change not applied (got: $NEW_LEVEL)"
    fi
    
    # Test agent log level change
    RESPONSE=$(curl -s -X PUT http://localhost:14001/api/v1/log/level \
        -H "Content-Type: application/json" \
        -d '{"level":"trace"}')
    
    if echo "$RESPONSE" | jq -e '.message' > /dev/null 2>&1; then
        pass_test "Changed agent log level to TRACE"
    else
        fail_test "Failed to change agent log level"
    fi
    
    # Verify agent change
    sleep 1
    AGENT_LEVEL=$(curl -s http://localhost:14001/api/v1/log/config | jq -r '.level')
    if [ "$AGENT_LEVEL" = "trace" ]; then
        pass_test "Agent log level change verified"
    else
        fail_test "Agent log level change not applied (got: $AGENT_LEVEL)"
    fi
}

# Test 4: Log retention configuration
test_log_retention() {
    info "Test 4: Log retention configuration"
    
    # Update server retention
    RESPONSE=$(curl -s -X PUT http://localhost:14000/api/v1/log/retention \
        -H "Content-Type: application/json" \
        -d '{"retention_count":5}')
    
    if echo "$RESPONSE" | jq -e '.message' > /dev/null 2>&1; then
        pass_test "Updated server log retention to 5 days"
    else
        fail_test "Failed to update server log retention"
        return 1
    fi
    
    # Verify the change
    sleep 1
    RETENTION=$(curl -s http://localhost:14000/api/v1/log/config | jq -r '.retention_count')
    if [ "$RETENTION" = "5" ]; then
        pass_test "Server log retention change verified"
    else
        fail_test "Server log retention change not applied (got: $RETENTION)"
    fi
    
    # Update agent retention
    RESPONSE=$(curl -s -X PUT http://localhost:14001/api/v1/log/retention \
        -H "Content-Type: application/json" \
        -d '{"retention_count":10}')
    
    if echo "$RESPONSE" | jq -e '.message' > /dev/null 2>&1; then
        pass_test "Updated agent log retention to 10 days"
    else
        fail_test "Failed to update agent log retention"
    fi
    
    # Verify agent change
    sleep 1
    AGENT_RETENTION=$(curl -s http://localhost:14001/api/v1/log/config | jq -r '.retention_count')
    if [ "$AGENT_RETENTION" = "10" ]; then
        pass_test "Agent log retention change verified"
    else
        fail_test "Agent log retention change not applied (got: $AGENT_RETENTION)"
    fi
}

# Test 5: Log file structure and format
test_log_format() {
    info "Test 5: Log file structure and format"
    
    # Check server log format
    if grep -E "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}" /tmp/opsbox-test-logs/opsbox-server.log > /dev/null 2>&1; then
        pass_test "Server log contains ISO 8601 timestamps"
    else
        fail_test "Server log missing proper timestamps"
    fi
    
    if grep -E "(ERROR|WARN|INFO|DEBUG|TRACE)" /tmp/opsbox-test-logs/opsbox-server.log > /dev/null 2>&1; then
        pass_test "Server log contains log levels"
    else
        fail_test "Server log missing log levels"
    fi
    
    # Check agent log format
    if grep -E "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}" /tmp/opsbox-agent-test-logs/opsbox-agent.log > /dev/null 2>&1; then
        pass_test "Agent log contains ISO 8601 timestamps"
    else
        fail_test "Agent log missing proper timestamps"
    fi
    
    if grep -E "(ERROR|WARN|INFO|DEBUG|TRACE)" /tmp/opsbox-agent-test-logs/opsbox-agent.log > /dev/null 2>&1; then
        pass_test "Agent log contains log levels"
    else
        fail_test "Agent log missing log levels"
    fi
}

# Test 6: Generate activity to test logging
test_logging_activity() {
    info "Test 6: Logging activity under load"
    
    # Make several API calls to generate logs
    for i in {1..10}; do
        curl -s http://localhost:14000/api/v1/log/config > /dev/null
        curl -s http://localhost:14001/api/v1/log/config > /dev/null
    done
    
    sleep 2
    
    # Check if logs were written
    SERVER_LOG_SIZE=$(wc -l < /tmp/opsbox-test-logs/opsbox-server.log)
    AGENT_LOG_SIZE=$(wc -l < /tmp/opsbox-agent-test-logs/opsbox-agent.log)
    
    if [ "$SERVER_LOG_SIZE" -gt 10 ]; then
        pass_test "Server logged activity (${SERVER_LOG_SIZE} lines)"
    else
        fail_test "Server log too small (${SERVER_LOG_SIZE} lines)"
    fi
    
    if [ "$AGENT_LOG_SIZE" -gt 5 ]; then
        pass_test "Agent logged activity (${AGENT_LOG_SIZE} lines)"
    else
        fail_test "Agent log too small (${AGENT_LOG_SIZE} lines)"
    fi
}

# Test 7: Invalid parameter validation
test_parameter_validation() {
    info "Test 7: Parameter validation"
    
    # Test invalid log level
    RESPONSE=$(curl -s -w "\n%{http_code}" -X PUT http://localhost:14000/api/v1/log/level \
        -H "Content-Type: application/json" \
        -d '{"level":"invalid"}')
    
    HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
    if [ "$HTTP_CODE" = "400" ] || [ "$HTTP_CODE" = "422" ]; then
        pass_test "Server rejected invalid log level"
    else
        fail_test "Server accepted invalid log level (HTTP $HTTP_CODE)"
    fi
    
    # Test invalid retention count
    RESPONSE=$(curl -s -w "\n%{http_code}" -X PUT http://localhost:14000/api/v1/log/retention \
        -H "Content-Type: application/json" \
        -d '{"retention_count":-1}')
    
    HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
    if [ "$HTTP_CODE" = "400" ] || [ "$HTTP_CODE" = "422" ]; then
        pass_test "Server rejected invalid retention count"
    else
        fail_test "Server accepted invalid retention count (HTTP $HTTP_CODE)"
    fi
}

# Main test execution
main() {
    echo "=========================================="
    echo "  Tracing Logging System E2E Tests"
    echo "=========================================="
    echo ""
    
    # Check dependencies
    if ! command -v jq &> /dev/null; then
        echo -e "${RED}Error: jq is required but not installed${NC}"
        exit 1
    fi
    
    if ! command -v curl &> /dev/null; then
        echo -e "${RED}Error: curl is required but not installed${NC}"
        exit 1
    fi
    
    setup_test_env
    
    # Run tests
    test_server_startup || true
    test_agent_startup || true
    test_dynamic_log_level || true
    test_log_retention || true
    test_log_format || true
    test_logging_activity || true
    test_parameter_validation || true
    
    # Print summary
    echo ""
    echo "=========================================="
    echo "  Test Summary"
    echo "=========================================="
    echo -e "${GREEN}Passed: ${TESTS_PASSED}${NC}"
    echo -e "${RED}Failed: ${TESTS_FAILED}${NC}"
    echo "Total:  $((TESTS_PASSED + TESTS_FAILED))"
    echo ""
    
    if [ $TESTS_FAILED -eq 0 ]; then
        echo -e "${GREEN}All tests passed!${NC}"
        exit 0
    else
        echo -e "${RED}Some tests failed!${NC}"
        exit 1
    fi
}

main "$@"
