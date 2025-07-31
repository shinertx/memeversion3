#!/bin/bash

echo "📊 MemeSnipe v24 Production Health Monitor"
echo "==========================================="

while true; do
    clear
    echo "🔄 $(date) - System Status Check"
    echo "============================================"
    
    echo "📈 Docker Services Status:"
    docker compose ps --format "table {{.Service}}\t{{.Status}}\t{{.Ports}}"
    echo ""
    
    echo "📡 Redis Streams Health:"
    echo "Strategy Specs: $(docker compose exec -T redis redis-cli xlen strategy_specs 2>/dev/null || echo 'N/A')"
    echo "Price Events: $(docker compose exec -T redis redis-cli xlen events:price 2>/dev/null || echo 'N/A')"
    echo "Allocations: $(docker compose exec -T redis redis-cli xlen allocations_channel 2>/dev/null || echo 'N/A')"
    echo ""
    
    echo "💰 Recent Trade Activity:"
    docker compose exec -T postgres psql -U postgres -d meme_snipe_v25 -c "SELECT strategy_id, token_address, side, amount_usd, status, entry_time FROM trades ORDER BY id DESC LIMIT 3;" 2>/dev/null || echo "Database not ready"
    echo ""
    
    echo "🔥 System Resource Usage:"
    echo "Memory: $(free -h | grep Mem | awk '{print $3"/"$2}')"
    echo "Disk: $(df -h / | tail -1 | awk '{print $3"/"$2 " (" $5 " used)"}')"
    echo ""
    
    echo "📡 Service Endpoints:"
    if curl -s http://localhost:8080/api/health > /dev/null 2>&1; then
        echo "✅ Dashboard (8080): OK"
    else
        echo "❌ Dashboard (8080): DOWN"
    fi
    
    if curl -s http://localhost:9090/-/healthy > /dev/null 2>&1; then
        echo "✅ Prometheus (9090): OK"
    else
        echo "❌ Prometheus (9090): DOWN"
    fi
    
    if curl -s http://localhost:3000/api/health > /dev/null 2>&1; then
        echo "✅ Grafana (3000): OK"
    else
        echo "⚠️  Grafana (3000): CHECK"
    fi
    echo ""
    
    echo "🔍 Recent Logs (Last 2 lines):"
    echo "Strategy Factory:"
    docker compose logs --tail=2 strategy_factory 2>/dev/null | tail -2
    echo "Portfolio Manager:"
    docker compose logs --tail=2 portfolio_manager 2>/dev/null | tail -2
    echo "Executor:"
    docker compose logs --tail=2 executor 2>/dev/null | tail -2
    echo ""
    
    echo "Press Ctrl+C to stop monitoring"
    echo "Refreshing in 30 seconds..."
    echo "============================================"
    
    sleep 30
done
