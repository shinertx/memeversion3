#!/bin/bash

# Script to update VM .env with missing critical variables

echo "🔧 UPDATING VM .env WITH MISSING VARIABLES..."

# Add missing database configuration
gcloud compute ssh meme-snipe-v19-vm --zone=us-central1-a --command="
cd /opt/vm25 && 
echo '# Database Configuration - ADDED' >> .env && 
echo 'DATABASE_URL=postgresql://postgres:password@postgres:5432/meme_snipe_v25' >> .env && 
echo 'DB_PASSWORD=password' >> .env && 
echo '' >> .env
"

# Add missing genetic algorithm parameters
gcloud compute ssh meme-snipe-v19-vm --zone=us-central1-a --command="
cd /opt/vm25 && 
echo '# Genetic Algorithm Parameters - ADDED' >> .env && 
echo 'POPULATION_SIZE=50' >> .env && 
echo 'CROSSOVER_RATE=0.7' >> .env && 
echo 'MUTATION_RATE=0.1' >> .env && 
echo 'TOURNAMENT_SIZE=5' >> .env && 
echo '' >> .env
"

# Add missing strategy performance thresholds
gcloud compute ssh meme-snipe-v19-vm --zone=us-central1-a --command="
cd /opt/vm25 && 
echo '# Strategy Performance Thresholds - ADDED' >> .env && 
echo 'MIN_SHARPE_FOR_PROMOTION=1.5' >> .env && 
echo 'STRATEGY_PROMOTION_INTERVAL_SECS=3600' >> .env && 
echo '' >> .env
"

# Update Farcaster API key to real one
gcloud compute ssh meme-snipe-v19-vm --zone=us-central1-a --command="
cd /opt/vm25 && 
sed -i 's/FARCASTER_API_KEY=.*/FARCASTER_API_KEY=F46B6C02-351C-478E-81DA-1A8561CDB790/' .env
"

echo "✅ VM .env UPDATED WITH MISSING VARIABLES"
echo "🔍 CHECKING UPDATED .env ON VM..."

# Show the updated .env file
gcloud compute ssh meme-snipe-v19-vm --zone=us-central1-a --command="
cd /opt/vm25 && 
echo '=== UPDATED .env FILE ===' &&
tail -20 .env
"
