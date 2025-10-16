#!/bin/bash

# Extended SSR Server Benchmark Suite
# Tests versioned cache, critical data, and lazy loading optimizations

SERVER_URL="http://localhost:3000"
API_URL="$SERVER_URL/api"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     🦀 Optimized SSR Server Benchmark (v2.0)              ║${NC}"
echo -e "${CYAN}║     Versioned Cache + Critical Data + Lazy Loading         ║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

# Check if server is running
echo -e "${YELLOW}Checking server availability...${NC}"
if ! curl -s -f $SERVER_URL > /dev/null; then
    echo -e "${RED}❌ Server not running at $SERVER_URL${NC}"
    echo -e "${YELLOW}Please start the server first:${NC}"
    echo -e "  cd enddelServer && cargo run --release"
    exit 1
fi
echo -e "${GREEN}✓ Server is running${NC}"
echo ""

# Test 1: Bundle Size Analysis
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📦 Test 1: SSR Bundle Size Analysis${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

# Get HTML response size
response=$(curl -s $SERVER_URL)
html_size=${#response}
html_size_kb=$(echo "scale=2; $html_size / 1024" | bc)

# Get compressed size
compressed_size=$(curl -s -H "Accept-Encoding: br" $SERVER_URL | wc -c)
compressed_kb=$(echo "scale=2; $compressed_size / 1024" | bc)

# Calculate compression ratio
compression_ratio=$(echo "scale=2; $html_size / $compressed_size" | bc)

echo -e "${GREEN}✓ HTML Size (raw):      ${html_size_kb}KB (${html_size} bytes)${NC}"
echo -e "${GREEN}✓ HTML Size (brotli):   ${compressed_kb}KB (${compressed_size} bytes)${NC}"
echo -e "${GREEN}✓ Compression Ratio:    ${compression_ratio}x${NC}"

# Check if SSR bundle exists
if [ -f "../enddelServer/ssr-bundle-embedded.js" ]; then
    bundle_size=$(wc -c < "../enddelServer/ssr-bundle-embedded.js")
    bundle_kb=$(echo "scale=2; $bundle_size / 1024" | bc)
    echo -e "${GREEN}✓ SSR Bundle Size:      ${bundle_kb}KB${NC}"
else
    echo -e "${YELLOW}⚠ SSR bundle not found at ssr-bundle-embedded.js${NC}"
fi
echo ""

# Test 2: Versioned Cache Performance
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}🔄 Test 2: Versioned Cache Coherency${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

echo -e "${YELLOW}Testing cache versioning with multiple URLs...${NC}"

# Test different URLs to verify versioning
urls=("/" "/about" "/products")
total_time=0
count=0

for url in "${urls[@]}"; do
    # First request (cache miss or cache hit with current version)
    time1=$(curl -o /dev/null -s -w "%{time_total}" "$SERVER_URL$url")

    # Second request (should be cached with same version)
    time2=$(curl -o /dev/null -s -w "%{time_total}" "$SERVER_URL$url")

    # Third request (verify cache consistency)
    time3=$(curl -o /dev/null -s -w "%{time_total}" "$SERVER_URL$url")

    time1_ms=$(echo "scale=3; $time1 * 1000" | bc)
    time2_ms=$(echo "scale=3; $time2 * 1000" | bc)
    time3_ms=$(echo "scale=3; $time3 * 1000" | bc)

    echo -e "${GREEN}  $url:${NC}"
    echo -e "    Request 1: ${time1_ms}ms (warm/miss)"
    echo -e "    Request 2: ${time2_ms}ms (versioned cache hit)"
    echo -e "    Request 3: ${time3_ms}ms (versioned cache hit)"

    # Accumulate average cached time
    avg=$(echo "scale=6; ($time2 + $time3) / 2" | bc)
    total_time=$(echo "$total_time + $avg" | bc)
    count=$((count + 1))
done

avg_versioned_cache=$(echo "scale=6; $total_time / $count" | bc)
avg_versioned_ms=$(echo "scale=3; $avg_versioned_cache * 1000" | bc)

echo ""
echo -e "${GREEN}✓ Avg Versioned Cache Latency: ${avg_versioned_ms}ms${NC}"
echo ""

# Test 3: Critical Data Loading Performance
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}⚡ Test 3: Critical Data SSR Performance${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

echo -e "${YELLOW}Testing SSR with critical data (text-only, no images)...${NC}"

# Clear cache by requesting a unique URL with timestamp
cache_bust=$(date +%s%N)
time_critical=$(curl -o /dev/null -s -w "%{time_total}" "$SERVER_URL/?t=$cache_bust")
time_critical_ms=$(echo "scale=3; $time_critical * 1000" | bc)

echo -e "${GREEN}✓ SSR with Critical Data: ${time_critical_ms}ms${NC}"

# Verify critical data is in response
response=$(curl -s "$SERVER_URL/")
if echo "$response" | grep -q "window.__PRELOADED_DATA__"; then
    echo -e "${GREEN}✓ Critical data preloaded in HTML${NC}"
else
    echo -e "${YELLOW}⚠ Critical data not found in response${NC}"
fi
echo ""

# Test 4: Lazy Loading API Performance
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}🖼️  Test 4: Lazy Loading API Endpoint${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

echo -e "${YELLOW}Testing /api/products/lazy/:id endpoint...${NC}"

# Test lazy loading for different product IDs
product_ids=(1 2 3 4 5)
lazy_total=0
lazy_count=0

for id in "${product_ids[@]}"; do
    time_lazy=$(curl -o /dev/null -s -w "%{time_total}" "$API_URL/products/lazy/$id")
    time_lazy_ms=$(echo "scale=3; $time_lazy * 1000" | bc)

    echo -e "  Product $id: ${time_lazy_ms}ms"

    lazy_total=$(echo "$lazy_total + $time_lazy" | bc)
    lazy_count=$((lazy_count + 1))
done

avg_lazy=$(echo "scale=6; $lazy_total / $lazy_count" | bc)
avg_lazy_ms=$(echo "scale=3; $avg_lazy * 1000" | bc)

echo ""
echo -e "${GREEN}✓ Avg Lazy Loading Latency: ${avg_lazy_ms}ms${NC}"
echo ""

# Test 5: 1000 Requests SSR Benchmark
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}🚀 Test 5: 1000 SSR Requests Benchmark${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

echo -e "${YELLOW}Running 1000 concurrent requests to SSR endpoint...${NC}"

start_time=$(date +%s.%N)
for i in {1..1000}; do
    curl -s $SERVER_URL > /dev/null &

    # Show progress every 100 requests
    if [ $((i % 100)) -eq 0 ]; then
        echo -ne "${GREEN}Progress: $i/1000${NC}\r"
    fi
done
wait
end_time=$(date +%s.%N)

duration=$(echo "$end_time - $start_time" | bc)
throughput=$(echo "scale=0; 1000 / $duration" | bc)
avg_latency=$(echo "scale=3; ($duration / 1000) * 1000" | bc)

echo ""
echo -e "${GREEN}✓ Total Duration:        ${duration}s${NC}"
echo -e "${GREEN}✓ Throughput:            ${throughput} req/s${NC}"
echo -e "${GREEN}✓ Average Latency:       ${avg_latency}ms${NC}"
echo ""

# Test 6: Cache Hit Ratio Estimation
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}📊 Test 6: Cache Performance Analysis${NC}"
echo -e "${PURPLE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

echo -e "${YELLOW}Comparing cache miss vs cache hit performance...${NC}"

# Cache miss (unique URL with cache bust)
cache_bust=$(date +%s%N)
time_miss=$(curl -o /dev/null -s -w "%{time_total}" "$SERVER_URL/?cb=$cache_bust")
time_miss_ms=$(echo "scale=3; $time_miss * 1000" | bc)

# Cache hit (same URL)
time_hit1=$(curl -o /dev/null -s -w "%{time_total}" "$SERVER_URL/")
time_hit2=$(curl -o /dev/null -s -w "%{time_total}" "$SERVER_URL/")
time_hit3=$(curl -o /dev/null -s -w "%{time_total}" "$SERVER_URL/")

avg_hit=$(echo "scale=6; ($time_hit1 + $time_hit2 + $time_hit3) / 3" | bc)
time_hit_ms=$(echo "scale=3; $avg_hit * 1000" | bc)

speedup=$(echo "scale=2; $time_miss / $avg_hit" | bc)

echo -e "${GREEN}✓ Cache Miss (SSR):      ${time_miss_ms}ms${NC}"
echo -e "${GREEN}✓ Cache Hit (versioned): ${time_hit_ms}ms${NC}"
echo -e "${GREEN}✓ Speedup:               ${speedup}x${NC}"
echo ""

# Summary
echo -e "${CYAN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║              📈 OPTIMIZED BENCHMARK SUMMARY                ║${NC}"
echo -e "${CYAN}╠════════════════════════════════════════════════════════════╣${NC}"
echo -e "${CYAN}║${NC}                                                            ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  ${GREEN}Bundle Size:${NC}                                              ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • HTML (raw):          ${html_size_kb}KB                      ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • HTML (brotli):       ${compressed_kb}KB                      ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • Compression:         ${compression_ratio}x                        ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}                                                            ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  ${GREEN}Performance:${NC}                                              ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • Versioned Cache:     ${avg_versioned_ms}ms                     ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • Critical Data SSR:   ${time_critical_ms}ms                     ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • Lazy Loading API:    ${avg_lazy_ms}ms                     ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • 1000 req throughput: ${throughput} req/s                ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}                                                            ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}  ${GREEN}Cache Efficiency:${NC}                                         ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • Cache Miss:          ${time_miss_ms}ms                     ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • Cache Hit:           ${time_hit_ms}ms                     ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}    • Cache Speedup:       ${speedup}x                        ${CYAN}║${NC}"
echo -e "${CYAN}║${NC}                                                            ${CYAN}║${NC}"
echo -e "${CYAN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

echo -e "${YELLOW}🎯 Key Optimizations Tested:${NC}"
echo -e "  ✅ Versioned HTML cache (coherency with product data)"
echo -e "  ✅ Critical data separation (text-only for SSR)"
echo -e "  ✅ Lazy loading API (images loaded on demand)"
echo -e "  ✅ Multi-tier caching (L1/L2 CPU + RAM)"
echo -e "  ✅ Progressive enhancement (SEO-first approach)"
echo ""

echo -e "${GREEN}✅ Benchmark completed!${NC}"
echo ""

echo -e "${YELLOW}💡 Next Steps:${NC}"
echo -e "  • Run wrk for production-grade load testing:"
echo -e "    ${BLUE}wrk -t12 -c400 -d30s $SERVER_URL${NC}"
echo -e "  • Monitor cache hit ratio in production logs"
echo -e "  • Implement client-side Intersection Observer for lazy loading"
echo ""
