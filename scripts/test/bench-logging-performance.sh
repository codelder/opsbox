#!/bin/bash
# Performance Benchmarking Script for Tracing Logging System
# Tests throughput, latency, CPU, memory, and disk I/O

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
TEST_DURATION=30
CONCURRENT_REQUESTS=50
LOG_DIR="/tmp/opsbox-perf-test-logs"
DB_PATH="/tmp/opsbox-perf-test-db"
SERVER_PORT=14100

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    if [ ! -z "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
    fi
    rm -rf "$LOG_DIR"
    rm -rf "$DB_PATH"
}

trap cleanup EXIT

info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

success() {
    echo -e "${GREEN}✓${NC} $1"
}

warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

error() {
    echo -e "${RED}✗${NC} $1"
}

# Setup test environment
setup() {
    info "Setting up performance test environment..."
    mkdir -p "$LOG_DIR"
    mkdir -p "$DB_PATH"
    
    # Build in release mode
    info "Building server in release mode..."
    cargo build --release --bin opsbox-server 2>&1 | tail -5
    success "Build complete"
}

# Start server
start_server() {
    local log_level=$1
    info "Starting server with log level: $log_level"
    
    ./target/release/opsbox-server \
        --log-dir "$LOG_DIR" \
        --log-retention 7 \
        --db-path "$DB_PATH/server.db" \
        --listen-port $SERVER_PORT \
        > /dev/null 2>&1 &
    SERVER_PID=$!
    
    # Wait for server to start
    sleep 3
    
    # Set log level
    curl -s -X PUT http://localhost:$SERVER_PORT/api/v1/log/level \
        -H "Content-Type: application/json" \
        -d "{\"level\":\"$log_level\"}" > /dev/null
    
    if ! kill -0 $SERVER_PID 2>/dev/null; then
        error "Server failed to start"
        exit 1
    fi
    
    success "Server started (PID: $SERVER_PID)"
}

# Stop server
stop_server() {
    if [ ! -z "$SERVER_PID" ]; then
        info "Stopping server..."
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
        SERVER_PID=""
        sleep 2
    fi
}

# Benchmark throughput
benchmark_throughput() {
    local log_level=$1
    info "Benchmarking throughput at $log_level level..."
    
    # Use Apache Bench if available, otherwise use curl loop
    if command -v ab &> /dev/null; then
        ab -n 10000 -c $CONCURRENT_REQUESTS \
            -q \
            http://localhost:$SERVER_PORT/api/v1/log/config 2>&1 | \
            grep -E "(Requests per second|Time per request|Failed requests)"
    else
        # Fallback to curl
        local start_time=$(date +%s)
        local count=0
        
        for i in {1..1000}; do
            curl -s http://localhost:$SERVER_PORT/api/v1/log/config > /dev/null &
            ((count++))
            
            # Limit concurrent requests
            if [ $((count % CONCURRENT_REQUESTS)) -eq 0 ]; then
                wait
            fi
        done
        wait
        
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        local rps=$((1000 / duration))
        
        echo "Requests: 1000"
        echo "Duration: ${duration}s"
        echo "Requests per second: $rps"
    fi
}

# Measure resource usage
measure_resources() {
    local log_level=$1
    local duration=$2
    info "Measuring resource usage for ${duration}s at $log_level level..."
    
    # Start monitoring in background
    local cpu_samples=()
    local mem_samples=()
    
    for i in $(seq 1 $duration); do
        if kill -0 $SERVER_PID 2>/dev/null; then
            # Get CPU and memory usage (macOS compatible)
            if [[ "$OSTYPE" == "darwin"* ]]; then
                local ps_output=$(ps -p $SERVER_PID -o %cpu,rss | tail -1)
                local cpu=$(echo $ps_output | awk '{print $1}')
                local mem=$(echo $ps_output | awk '{print $2}')
                mem=$((mem / 1024)) # Convert to MB
            else
                local ps_output=$(ps -p $SERVER_PID -o %cpu,rss | tail -1)
                local cpu=$(echo $ps_output | awk '{print $1}')
                local mem=$(echo $ps_output | awk '{print $2}')
                mem=$((mem / 1024)) # Convert to MB
            fi
            
            cpu_samples+=($cpu)
            mem_samples+=($mem)
        fi
        
        # Generate some load
        for j in {1..10}; do
            curl -s http://localhost:$SERVER_PORT/api/v1/log/config > /dev/null &
        done
        
        sleep 1
    done
    wait
    
    # Calculate averages
    local cpu_sum=0
    local mem_sum=0
    local count=${#cpu_samples[@]}
    
    for cpu in "${cpu_samples[@]}"; do
        cpu_sum=$(echo "$cpu_sum + $cpu" | bc)
    done
    
    for mem in "${mem_samples[@]}"; do
        mem_sum=$((mem_sum + mem))
    done
    
    local avg_cpu=$(echo "scale=2; $cpu_sum / $count" | bc)
    local avg_mem=$((mem_sum / count))
    local max_mem=$(printf '%s\n' "${mem_samples[@]}" | sort -n | tail -1)
    
    echo "Average CPU: ${avg_cpu}%"
    echo "Average Memory: ${avg_mem} MB"
    echo "Peak Memory: ${max_mem} MB"
}

# Measure disk I/O
measure_disk_io() {
    local log_level=$1
    info "Measuring disk I/O at $log_level level..."
    
    # Get initial log file size
    local initial_size=0
    if [ -f "$LOG_DIR/opsbox-server.log" ]; then
        initial_size=$(stat -f%z "$LOG_DIR/opsbox-server.log" 2>/dev/null || stat -c%s "$LOG_DIR/opsbox-server.log" 2>/dev/null || echo 0)
    fi
    
    # Generate load for 10 seconds
    local start_time=$(date +%s)
    for i in {1..100}; do
        curl -s http://localhost:$SERVER_PORT/api/v1/log/config > /dev/null &
    done
    wait
    local end_time=$(date +%s)
    
    # Get final log file size
    local final_size=$(stat -f%z "$LOG_DIR/opsbox-server.log" 2>/dev/null || stat -c%s "$LOG_DIR/opsbox-server.log" 2>/dev/null || echo 0)
    
    local bytes_written=$((final_size - initial_size))
    local kb_written=$((bytes_written / 1024))
    local duration=$((end_time - start_time))
    local kb_per_sec=$((kb_written / duration))
    
    echo "Bytes written: $bytes_written"
    echo "KB written: $kb_written KB"
    echo "Duration: ${duration}s"
    echo "Write rate: ${kb_per_sec} KB/s"
}

# Test log levels comparison
compare_log_levels() {
    info "Comparing performance across log levels..."
    echo ""
    echo "=========================================="
    echo "  Performance Comparison by Log Level"
    echo "=========================================="
    echo ""
    
    for level in error warn info debug trace; do
        echo -e "${YELLOW}Testing $level level...${NC}"
        
        # Clean up previous test
        stop_server
        rm -rf "$LOG_DIR"/*
        mkdir -p "$LOG_DIR"
        
        # Start server with this level
        start_server $level
        
        # Run benchmarks
        echo ""
        echo "--- Throughput ---"
        benchmark_throughput $level
        
        echo ""
        echo "--- Resource Usage (10s) ---"
        measure_resources $level 10
        
        echo ""
        echo "--- Disk I/O ---"
        measure_disk_io $level
        
        echo ""
        echo "=========================================="
        echo ""
        
        # Stop server
        stop_server
    done
}

# Test concurrent logging
test_concurrent_logging() {
    info "Testing concurrent logging performance..."
    
    start_server "info"
    
    echo ""
    echo "--- High Concurrency Test ---"
    echo "Concurrent requests: $CONCURRENT_REQUESTS"
    echo "Duration: ${TEST_DURATION}s"
    echo ""
    
    local start_time=$(date +%s)
    local request_count=0
    
    # Generate high concurrent load
    while [ $(($(date +%s) - start_time)) -lt $TEST_DURATION ]; do
        for i in $(seq 1 $CONCURRENT_REQUESTS); do
            curl -s http://localhost:$SERVER_PORT/api/v1/log/config > /dev/null &
            ((request_count++))
        done
        wait
    done
    
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    local rps=$((request_count / duration))
    
    echo "Total requests: $request_count"
    echo "Duration: ${duration}s"
    echo "Requests per second: $rps"
    
    # Check for errors in logs
    local error_count=$(grep -c "ERROR" "$LOG_DIR/opsbox-server.log" 2>/dev/null || echo 0)
    echo "Errors logged: $error_count"
    
    if [ $error_count -eq 0 ]; then
        success "No errors during concurrent load"
    else
        warning "$error_count errors found in logs"
    fi
    
    stop_server
}

# Memory leak test
test_memory_leak() {
    info "Testing for memory leaks..."
    
    start_server "info"
    
    echo ""
    echo "--- Memory Leak Test (60s) ---"
    echo ""
    
    local samples=60
    local mem_readings=()
    
    for i in $(seq 1 $samples); do
        # Generate some load
        for j in {1..20}; do
            curl -s http://localhost:$SERVER_PORT/api/v1/log/config > /dev/null &
        done
        wait
        
        # Sample memory
        if kill -0 $SERVER_PID 2>/dev/null; then
            if [[ "$OSTYPE" == "darwin"* ]]; then
                local mem=$(ps -p $SERVER_PID -o rss | tail -1)
                mem=$((mem / 1024))
            else
                local mem=$(ps -p $SERVER_PID -o rss | tail -1)
                mem=$((mem / 1024))
            fi
            mem_readings+=($mem)
            
            if [ $((i % 10)) -eq 0 ]; then
                echo "Sample $i: ${mem} MB"
            fi
        fi
        
        sleep 1
    done
    
    # Analyze memory trend
    local first_mem=${mem_readings[0]}
    local last_mem=${mem_readings[-1]}
    local mem_growth=$((last_mem - first_mem))
    local growth_percent=$(echo "scale=2; ($mem_growth * 100) / $first_mem" | bc)
    
    echo ""
    echo "Initial memory: ${first_mem} MB"
    echo "Final memory: ${last_mem} MB"
    echo "Growth: ${mem_growth} MB (${growth_percent}%)"
    
    if [ $mem_growth -lt 50 ]; then
        success "Memory usage stable (growth < 50 MB)"
    else
        warning "Significant memory growth detected: ${mem_growth} MB"
    fi
    
    stop_server
}

# Main execution
main() {
    echo "=========================================="
    echo "  Tracing Logging Performance Tests"
    echo "=========================================="
    echo ""
    
    # Check dependencies
    if ! command -v bc &> /dev/null; then
        warning "bc not installed, some calculations may be limited"
    fi
    
    setup
    
    # Run tests
    compare_log_levels
    test_concurrent_logging
    test_memory_leak
    
    echo ""
    echo "=========================================="
    echo "  Performance Testing Complete"
    echo "=========================================="
    echo ""
    
    success "All performance tests completed"
    echo ""
    echo "Summary:"
    echo "- Tested all log levels (ERROR, WARN, INFO, DEBUG, TRACE)"
    echo "- Measured throughput, CPU, memory, and disk I/O"
    echo "- Tested concurrent logging performance"
    echo "- Checked for memory leaks"
    echo ""
    echo "Review the output above for detailed metrics."
}

main "$@"
