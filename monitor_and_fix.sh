#!/bin/bash

# MemeSnipe v25 - Live System Monitor & Auto-Fix Script
# This script continuously monitors the system and fixes issues automatically

LOG_FILE="/opt/vm25/monitor.log"
ITERATION=0

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

check_service_status() {
    local service=$1
    local status=$(docker ps --filter "name=$service" --format "{{.Status}}")
    echo "$status"
}

fix_restarting_service() {
    local service=$1
    log "🔧 FIXING: $service is restarting - rebuilding..."
    
    case $service in
        "vm25-executor-1")
            log "🏗️ Rebuilding executor..."
            cd /opt/vm25
            docker compose build executor --no-cache
            docker compose up -d executor
            ;;
        "vm25-signer-1")
            log "🔑 Restarting signer..."
            cd /opt/vm25
            docker compose restart signer
            ;;
        *)
            log "🔄 Restarting $service..."
            cd /opt/vm25
            docker compose restart ${service#vm25-}
            ;;
    esac
}

check_and_fix_system() {
    ITERATION=$((ITERATION + 1))
    log "🔍 === ITERATION $ITERATION - System Health Check ==="
    
    # Check critical services
    EXECUTOR_STATUS=$(check_service_status "vm25-executor-1")
    SIGNER_STATUS=$(check_service_status "vm25-signer-1")
    PORTFOLIO_STATUS=$(check_service_status "vm25-portfolio_manager-1")
    REDIS_STATUS=$(check_service_status "vm25-redis-1")
    
    log "📊 Service Status:"
    log "   Executor: $EXECUTOR_STATUS"
    log "   Signer: $SIGNER_STATUS"
    log "   Portfolio: $PORTFOLIO_STATUS"
    log "   Redis: $REDIS_STATUS"
    
    # Check data flow
    PRICE_EVENTS=$(docker exec vm25-redis-1 redis-cli XLEN events:price 2>/dev/null || echo "ERROR")
    STRATEGY_SPECS=$(docker exec vm25-redis-1 redis-cli XLEN strategy_specs 2>/dev/null || echo "ERROR")
    ALLOCATIONS=$(docker exec vm25-redis-1 redis-cli XLEN allocations_channel 2>/dev/null || echo "ERROR")
    
    log "📈 Data Flow:"
    log "   Market Events: $PRICE_EVENTS"
    log "   Strategy Specs: $STRATEGY_SPECS"
    log "   Capital Allocations: $ALLOCATIONS"
    
    # Auto-fix restarting services
    if [[ "$EXECUTOR_STATUS" == *"Restarting"* ]]; then
        fix_restarting_service "vm25-executor-1"
    fi
    
    if [[ "$SIGNER_STATUS" == *"Restarting"* ]]; then
        fix_restarting_service "vm25-signer-1"
    fi
    
    # Check if executor is finally UP
    if [[ "$EXECUTOR_STATUS" == *"Up"* ]]; then
        log "🎉 SUCCESS! Executor is UP and running!"
        log "💰 System should now be allocating capital to strategies!"
        
        # Check if allocations started
        if [[ "$ALLOCATIONS" != "0" && "$ALLOCATIONS" != "ERROR" ]]; then
            log "🚀 TRADING PIPELINE ACTIVE! Capital allocations detected: $ALLOCATIONS"
        fi
    fi
    
    # Check build progress if executor is still building
    if [[ "$EXECUTOR_STATUS" == *"Restarting"* ]]; then
        log "🏗️ Executor still building/restarting - checking latest logs..."
        docker logs vm25-executor-1 --tail=3 2>/dev/null | while read line; do
            log "   📋 $line"
        done
    fi
    
    log "✅ Check complete. Next check in 30 seconds..."
    echo "----------------------------------------"
}

# Main monitoring loop
log "🚀 Starting MemeSnipe v25 Live Monitor..."
log "📍 Working directory: $(pwd)"
log "🎯 Mission: Keep the trading system ALIVE and PROFITABLE!"

while true; do
    check_and_fix_system
    sleep 30
done
