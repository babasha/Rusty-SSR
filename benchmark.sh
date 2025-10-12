#!/bin/bash

# SSR Server Benchmark Suite
# Tests cache performance, concurrency, and latency

SERVER_URL="http://localhost:3000"
RESULTS_DIR="benchmark-results"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║        🦀 Rust SSR Server Performance Benchmark           ║${NC}"
echo -e "${CYAN}║           Multi-tier Cache + V8 Pool Test Suite            ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Test 1: Single Request Latency
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📊 Test 1: Single Request Latency (Cache Hit)${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Warm up cache
curl -s $SERVER_URL > /dev/null

total=0
count=100
echo -e "${YELLOW}Running $count sequential requests...${NC}"
for i in $(seq 1 $count); do
    time=$(curl -o /dev/null -s -w "%{time_total}" $SERVER_URL)
    total=$(echo "$total + $time" | bc)
    if [ $((i % 20)) -eq 0 ]; then
        echo -ne "${GREEN}Progress: $i/$count${NC}\r"
    fi
done
echo ""

avg=$(echo "scale=6; $total / $count" | bc)
avg_ms=$(echo "scale=3; $avg * 1000" | bc)
rps=$(echo "scale=0; 1 / $avg" | bc)

echo -e "${GREEN}✓ Average Latency:   ${avg_ms}ms${NC}"
echo -e "${GREEN}✓ Requests/sec:      ~${rps} req/s${NC}"
echo ""

# Test 2: Concurrent Requests
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📊 Test 2: Concurrent Requests (1000 parallel)${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

echo -e "${YELLOW}Launching 1000 concurrent curl requests...${NC}"
start_time=$(date +%s.%N)
for i in {1..1000}; do
    curl -s $SERVER_URL > /dev/null &
done
wait
end_time=$(date +%s.%N)

duration=$(echo "$end_time - $start_time" | bc)
throughput=$(echo "scale=0; 1000 / $duration" | bc)

echo -e "${GREEN}✓ Duration:          ${duration}s${NC}"
echo -e "${GREEN}✓ Throughput:        ~${throughput} req/s${NC}"
echo ""

# Test 3: Sustained Load
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📊 Test 3: Sustained Load (10,000 requests)${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

echo -e "${YELLOW}Running 10,000 requests in batches of 100...${NC}"
start_time=$(date +%s.%N)

for batch in {1..100}; do
    for i in {1..100}; do
        curl -s $SERVER_URL > /dev/null &
    done
    wait
    echo -ne "${GREEN}Progress: $((batch * 100))/10000${NC}\r"
done
echo ""

end_time=$(date +%s.%N)
duration=$(echo "$end_time - $start_time" | bc)
throughput=$(echo "scale=0; 10000 / $duration" | bc)

echo -e "${GREEN}✓ Duration:          ${duration}s${NC}"
echo -e "${GREEN}✓ Total Requests:    10,000${NC}"
echo -e "${GREEN}✓ Avg Throughput:    ~${throughput} req/s${NC}"
echo ""

# Test 4: Cache Performance
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📊 Test 4: Cache Hit vs Cold Start${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# First request (potentially cold)
time1=$(curl -o /dev/null -s -w "%{time_total}" $SERVER_URL)

# Cached request
time2=$(curl -o /dev/null -s -w "%{time_total}" $SERVER_URL)
time3=$(curl -o /dev/null -s -w "%{time_total}" $SERVER_URL)

time1_ms=$(echo "scale=3; $time1 * 1000" | bc)
time2_ms=$(echo "scale=3; $time2 * 1000" | bc)
time3_ms=$(echo "scale=3; $time3 * 1000" | bc)

echo -e "${GREEN}✓ Request 1 (warm):  ${time1_ms}ms${NC}"
echo -e "${GREEN}✓ Request 2 (cache): ${time2_ms}ms${NC}"
echo -e "${GREEN}✓ Request 3 (cache): ${time3_ms}ms${NC}"
echo ""

# Summary
echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║                    📈 BENCHMARK SUMMARY                    ║${NC}"
echo -e "${CYAN}╠════════════════════════════════════════════════════════════╣${NC}"
echo -e "${CYAN}║${NC}  ${GREEN}Average Latency:${NC}      ${avg_ms}ms                           ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  ${GREEN}Peak Throughput:${NC}      ~${throughput} req/s                   ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  ${GREEN}Cache Hit Latency:${NC}    ${time2_ms}ms                           ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  ${GREEN}Total Requests:${NC}       11,100                               ${CYAN}║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo -e "${YELLOW}Architecture:${NC}"
echo -e "  • Multi-tier cache: L1/L2 (thread-local) + RAM (shared)"
echo -e "  • V8 Thread Pool: $(sysctl -n hw.ncpu) workers"
echo -e "  • Auto-promotion: Cold → Hot cache on access"
echo -e "  • Zero-copy: Arc<str> for shared HTML"
echo ""
echo -e "${GREEN}✅ Benchmark completed!${NC}"
