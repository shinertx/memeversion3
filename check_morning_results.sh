#!/bin/bash
# MemeSnipe v25 Morning Results Checker
# Run this script tomorrow morning to get your P&L results

echo "🌅 CHECKING MEMESNIPE V25 OVERNIGHT RESULTS..."
echo "=============================================="

# Connect to VM and run morning results
ssh benjaminjones@35.192.102.93 'cd memeversion3 && ./morning_results.sh'

echo ""
echo "💡 To check results manually:"
echo "ssh benjaminjones@35.192.102.93 'cd memeversion3 && ./morning_results.sh'"
echo ""
echo "📊 Live Dashboards:"
echo "Main Dashboard: http://35.192.102.93:8080"
echo "Grafana: http://35.192.102.93:3000"
echo "Prometheus: http://35.192.102.93:9090"
