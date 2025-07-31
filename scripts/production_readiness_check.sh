#!/bin/bash
set -e

echo "🔍 MemeSnipe v24 Production Readiness Checklist"
echo "==============================================="

# Configuration Validation
echo "📋 Step 1: Configuration Validation"
echo "Checking .env file..."
if [ -f .env ]; then
    echo "✅ .env file exists"
    
    # Check critical environment variables
    source .env
    
    echo "Checking critical environment variables..."
    if [ "$PAPER_TRADING_MODE" = "true" ]; then
        echo "✅ PAPER_TRADING_MODE=true (Safe for testing)"
    else
        echo "⚠️  PAPER_TRADING_MODE=$PAPER_TRADING_MODE (LIVE TRADING ENABLED!)"
    fi
    
    if [ -n "$REDIS_URL" ]; then
        echo "✅ REDIS_URL configured"
    else
        echo "❌ REDIS_URL missing"
    fi
    
    if [ -n "$INITIAL_CAPITAL_USD" ]; then
        echo "✅ INITIAL_CAPITAL_USD=$INITIAL_CAPITAL_USD"
    else
        echo "❌ INITIAL_CAPITAL_USD missing"
    fi
else
    echo "❌ .env file missing - creating from template"
    cp .env.example .env
fi

# Code Quality Checks
echo "📋 Step 2: Code Quality Verification"
echo "Checking for unwrap() and expect() in critical paths..."
echo "Searching executor service..."
grep -r "unwrap()\|expect(" executor/src/ || echo "✅ No unwrap()/expect() found in executor"

echo "Searching portfolio manager..."
grep -r "unwrap()\|expect(" portfolio_manager/src/ || echo "✅ No unwrap()/expect() found in portfolio_manager"

echo "Searching market data gateway..."
grep -r "unwrap()\|expect(" market_data_gateway/src/ || echo "✅ No unwrap()/expect() found in market_data_gateway"

# Circuit Breaker Integration
echo "📋 Step 3: Circuit Breaker Integration Check"
echo "Checking for circuit breaker usage..."
grep -r "CircuitBreaker" executor/src/ && echo "✅ Circuit breaker integrated in executor" || echo "⚠️  Circuit breaker not found in executor"

# Database Schema Validation
echo "📋 Step 4: Database Schema Validation"
echo "Checking if database initialization is production-ready..."
grep -r "TODO" executor/src/database.rs && echo "⚠️  TODO items found in database code" || echo "✅ No TODO items in database code"

# Docker Compose Validation
echo "📋 Step 5: Docker Compose Configuration"
echo "Validating docker-compose.yml..."
if docker compose config > /dev/null 2>&1; then
    echo "✅ docker-compose.yml is valid"
else
    echo "❌ docker-compose.yml has syntax errors"
fi

# Resource Requirements Check
echo "📋 Step 6: System Resource Requirements"
echo "Available memory: $(free -h | grep Mem | awk '{print $7}')"
echo "Available disk: $(df -h / | tail -1 | awk '{print $4}')"
echo "Docker daemon status: $(systemctl is-active docker || echo 'unknown')"

# Port Availability Check
echo "📋 Step 7: Port Availability"
echo "Checking required ports..."
for port in 8080 9090 3000 6379 5432; do
    if lsof -i :$port > /dev/null 2>&1; then
        echo "⚠️  Port $port is occupied"
    else
        echo "✅ Port $port is available"
    fi
done

# Security Checklist
echo "📋 Step 8: Security Configuration"
echo "Checking wallet files..."
if [ -f my_wallet.json ]; then
    echo "✅ Wallet file exists"
    if grep -q '"demo"' my_wallet.json; then
        echo "⚠️  Using demo wallet (replace for live trading)"
    else
        echo "✅ Real wallet configured"
    fi
else
    echo "❌ Wallet file missing"
fi

echo ""
echo "🎉 Production Readiness Summary"
echo "=============================="
echo "✅ = Ready for production"
echo "⚠️  = Needs attention"
echo "❌ = Must fix before production"
echo ""
echo "🚀 To start the system: Run './scripts/production_build_and_run.sh'"
echo "📊 To monitor: Run './scripts/production_health_monitor.sh'"
echo "📖 For more details: See README.md"
echo ""
echo "⚠️  IMPORTANT: Always test thoroughly in paper trading mode first!"
