import asyncio
import json
import os
import random
import time
import uuid
import math
from dataclasses import dataclass, asdict
from typing import List, Dict, Any, Optional
import redis.asyncio as redis
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Internal backtesting configuration - no external dependencies
INTERNAL_BACKTEST_LOOKBACK_DAYS = 30
INTERNAL_BACKTEST_INITIAL_CAPITAL = 10000.0

STRATEGY_FAMILIES = [
    "momentum_5m", "mean_revert_1h", "social_buzz", "liquidity_migration",
    "perp_basis_arb", "dev_wallet_drain", "airdrop_rotation",
    "korean_time_burst", "bridge_inflow", "rug_pull_sniffer"
]

POPULATION_SIZE = 10
CROSSOVER_RATE = 0.7
TOP_SURVIVORS_RATIO = 0.2

@dataclass
class StrategyGenome:
    id: str
    family: str
    params: Dict[str, Any]
    fitness: float = 0.0

class StrategyFactory:
    def __init__(self):
        self.redis_client = None
        self.population: List[StrategyGenome] = []
        self.generation = 0
        # Remove external HTTP client - using internal backtesting only
        self.tournament_size = 3
        self.mutation_rate = 0.1
        self.population_size = POPULATION_SIZE
        self.crossover_rate = CROSSOVER_RATE

    def get_default_params(self, family):
        """Gets realistic default parameters for a given strategy family."""
        if family == "momentum_5m":
            return {"lookback": 5, "vol_multiplier": 2.0, "price_change_threshold": 0.05}
        elif family == "mean_revert_1h":
            return {"period_hours": 1, "z_score_threshold": 2.0}
        elif family == "social_buzz":
            return {"lookback_minutes": 10, "std_dev_threshold": 2.5}
        elif family == "liquidity_migration":
            return {"min_volume_migrate_usd": 50000.0}
        elif family == "perp_basis_arb":
            return {"basis_threshold_pct": 0.5}
        elif family == "dev_wallet_drain":
            return {"dev_balance_threshold_pct": 2.0}
        elif family == "airdrop_rotation":
            return {"min_new_holders": 100}
        elif family == "korean_time_burst":
            return {"volume_multiplier_threshold": 1.5}
        elif family == "bridge_inflow":
            return {"min_bridge_volume_usd": 100000.0}
        elif family == "rug_pull_sniffer":
            return {"price_drop_pct": 0.8, "volume_multiplier": 5.0}
        return {}

    def tournament_select(self) -> StrategyGenome:
        """Selects a strategy using tournament selection."""
        tournament = random.sample(self.population, self.tournament_size)
        return max(tournament, key=lambda genome: genome.fitness)

    def crossover(self, parent1: StrategyGenome, parent2: StrategyGenome) -> StrategyGenome:
        """Performs crossover between two parents to create a child."""
        # Only crossover if parents are from the same family
        if parent1.family != parent2.family:
            return parent1  # Return first parent if families don't match
            
        child_params = {}
        for key in parent1.params:
            if key in parent2.params:
                child_params[key] = parent1.params[key] if random.random() < 0.5 else parent2.params[key]
            else:
                child_params[key] = parent1.params[key]  # Use parent1's value if key missing
        
        # Generate unique ID for child
        import time
        child_id = f"{parent1.family}_cross_{int(time.time())}_{random.randint(1000, 9999)}"
        return StrategyGenome(id=child_id, family=parent1.family, params=child_params)

    def mutate(self, genome: StrategyGenome) -> StrategyGenome:
        """Mutates a strategy's parameters."""
        for key, value in genome.params.items():
            if random.random() < self.mutation_rate:
                if isinstance(value, int):
                    genome.params[key] = max(1, value + random.randint(-5, 5))
                elif isinstance(value, float):
                    genome.params[key] = max(0.01, value + random.uniform(-0.1, 0.1))
        return genome

    def genome_to_spec(self, genome: StrategyGenome):
        """Convert a genome to a strategy specification."""
        return {
            "id": genome.id,
            "family": genome.family,
            "params": genome.params,
            "fitness": genome.fitness
        }

    async def calculate_internal_fitness(self, genome: StrategyGenome) -> float:
        """
        Internal backtesting engine - calculates fitness using simple heuristics.
        This replaces external API calls with local computation.
        """
        try:
            # Generate synthetic performance metrics based on strategy parameters
            # This is a simplified backtesting simulation for development
            
            base_fitness = 0.5  # Base Sharpe ratio
            
            if genome.family == "momentum_5m":
                # Momentum strategies perform better with higher vol multipliers
                vol_mult = genome.params.get("vol_multiplier", 2.0)
                base_fitness += min(0.3, vol_mult * 0.1)
                
            elif genome.family == "mean_revert_1h":
                # Mean reversion benefits from higher z-score thresholds
                z_threshold = genome.params.get("z_score_threshold", 2.0)
                base_fitness += min(0.4, z_threshold * 0.15)
                
            elif genome.family == "social_buzz":
                # Social strategies need balanced lookback periods
                lookback = genome.params.get("lookback_minutes", 10)
                if 5 <= lookback <= 15:
                    base_fitness += 0.3
                    
            elif genome.family == "liquidity_migration":
                # Volume-based strategies benefit from reasonable thresholds
                vol_threshold = genome.params.get("min_volume_migrate_usd", 50000.0)
                if 10000 <= vol_threshold <= 100000:
                    base_fitness += 0.25
                    
            # Add some randomness to simulate market uncertainty
            noise = random.uniform(-0.2, 0.2)
            final_fitness = max(0.1, min(3.0, base_fitness + noise))
            
            logger.info(f"Internal backtest for {genome.id}: base={base_fitness:.3f}, noise={noise:.3f}, final={final_fitness:.3f}")
            return final_fitness
            
        except Exception as e:
            logger.error(f"Error in internal fitness calculation for {genome.id}: {e}")
            return 0.5  # Default fitness on error

    async def evaluate_fitness(self):
        """Evaluate fitness of strategies using only internal backtesting."""
        logging.info("Evaluating fitness for current population using internal backtesting...")

        for genome in self.population:
            try:
                # Use internal backtesting calculation only
                fitness = await self.calculate_internal_fitness(genome)
                genome.fitness = fitness
                
                logging.info(f"Strategy {genome.id} internal backtest complete: fitness={genome.fitness:.3f}")
                
                # Publish result to Redis for portfolio_manager tracking
                await self.redis_client.xadd(
                    "backtest_results",
                    {
                        "strategy_id": genome.id, 
                        "fitness": str(fitness),
                        "backtest_type": "internal_simulation",
                        "spec": json.dumps(self.genome_to_spec(genome)),
                        "timestamp": str(int(time.time()))
                    }
                )

            except Exception as e:
                logging.error(f"Error evaluating fitness for {genome.id}: {e}")
                genome.fitness = 0.5

    async def evolve_population(self):
        """Evolves the strategy population using genetic algorithm principles."""
        logging.info(f"Starting population evolution. Current size: {len(self.population)}")
        
        # 1. Evaluate fitness of the current population
        await self.evaluate_fitness()

        new_population = []
        
        # Elitism: carry over the top N% of the population
        elite_count = int(self.population_size * 0.1) # Keep top 10%
        sorted_population = sorted(self.population, key=lambda g: g.fitness, reverse=True)
        new_population.extend(sorted_population[:elite_count])

        # 2. Generate the rest of the new population through crossover and mutation
        while len(new_population) < self.population_size:
            parent1 = self.tournament_select()
            parent2 = self.tournament_select()
            
            if random.random() < self.crossover_rate:
                child = self.crossover(parent1, parent2)
            else:
                child = parent1 # No crossover, just carry over a parent
            
            child = self.mutate(child)
            new_population.append(child)

        self.population = new_population
        logging.info(f"Population evolved. New size: {len(self.population)}")
        
        # 3. Publish the new generation of strategy specs to Redis
        await self.publish_strategy_specs()

    async def publish_strategy_specs(self):
        """Publish the current generation of strategy specs to Redis."""
        logging.info(f"Publishing {len(self.population)} strategy specs to Redis...")
        
        for genome in self.population:
            spec = self.genome_to_spec(genome)
            await self.redis_client.xadd(
                "strategy_specs",
                {"spec": json.dumps(spec)}
            )
            logging.info(f"Published strategy spec: {genome.id}")

async def main():
    factory = StrategyFactory()
    factory.redis_client = await redis.from_url("redis://redis:6379", decode_responses=True)
    
    logger.info("🚀 Starting MemeSnipe v25 Strategy Factory with Internal Backtesting")
    
    # Initial population - only create once
    if len(factory.population) == 0:
        logger.info("Creating initial population...")
        for family in STRATEGY_FAMILIES:
            for i in range(1, 6):  # 5 strategies per family
                params = factory.get_default_params(family)
                genome = StrategyGenome(
                    id=f"{family}_gen0_{i}_{int(time.time())}",
                    family=family,
                    params=params
                )
                factory.population.append(genome)
                spec = factory.genome_to_spec(genome)
                await factory.redis_client.xadd(
                    "strategy_specs",
                    {"spec": json.dumps(spec)}
                )
                logger.info(f"Proposed initial strategy: {genome.id}")
    
    # Continuous evolution loop
    generation = 0
    while True:
        try:
            generation += 1
            logger.info(f"🧬 Starting generation {generation} evolution...")
            
            await factory.evolve_population()
            
            logger.info(f"✅ Generation {generation} complete. Waiting 60 seconds before next evolution...")
            await asyncio.sleep(60)  # Evolve every minute
            
        except Exception as e:
            logger.error(f"Error in evolution cycle {generation}: {e}")
            await asyncio.sleep(30)  # Wait 30 seconds on error

if __name__ == "__main__":
    asyncio.run(main())
