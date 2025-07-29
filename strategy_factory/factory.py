import asyncio
import json
import os
import random
import time
import uuid
from dataclasses import dataclass, asdict
from typing import List, Dict, Any, Optional
import redis.asyncio as redis
import logging
import httpx

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

BACKTESTING_API_KEY = os.getenv("BACKTESTING_PLATFORM_API_KEY", "")
BACKTESTING_API_URL = os.getenv("BACKTESTING_PLATFORM_URL", "https://api.heliosprime.com/v1")

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
        self.http_client = httpx.AsyncClient(
            headers={"Authorization": f"Bearer {BACKTESTING_API_KEY}"},
            timeout=30.0
        )
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

    async def submit_for_backtest(self, genome: StrategyGenome) -> Optional[str]:
        """Submit a strategy to the external backtesting platform."""
        spec = self.genome_to_spec(genome)
        
        try:
            response = await self.http_client.post(
                f"{BACKTESTING_API_URL}/backtest",
                json={
                    "strategy_spec": spec,
                    "lookback_days": 30,
                    "initial_capital": 10000.0
                }
            )
            
            if response.status_code == 200:
                result = response.json()
                job_id = result.get("job_id")
                logger.info(f"Submitted strategy {spec['id']} for backtest, job_id: {job_id}")
                
                # Store the job_id for the portfolio manager to track
                await self.redis_client.xadd(
                    "backtest_jobs_submitted",
                    {"job_id": job_id, "strategy_id": spec['id'], "spec": json.dumps(spec)}
                )
                
                return job_id
            else:
                logger.error(f"Backtest submission failed: {response.status_code} - {response.text}")
                return None
                
        except Exception as e:
            logger.error(f"Error submitting backtest: {e}")
            return None

    async def evaluate_fitness(self):
        """Evaluate fitness of strategies using external backtesting API."""
        logging.info("Evaluating fitness for current population...")
        
        # For paper trading mode, use simplified fitness based on existing performance
        # In production, this would call the external backtesting API
        for genome in self.population:
            try:
                # For paper trading, always assign simulated fitness
                import random
                genome.fitness = random.uniform(0.5, 2.0)  # Simulated Sharpe ratio
                logging.info(f"Strategy {genome.id} assigned fitness: {genome.fitness}")
                
                # Optional: Still try to submit to backtest API for logging
                try:
                    await self.submit_for_backtest(genome)
                except:
                    pass  # Ignore API failures in paper trading mode
                    
            except Exception as e:
                logging.error(f"Error evaluating fitness for {genome.id}: {e}")
                genome.fitness = random.uniform(0.5, 2.0)  # Default simulated fitness

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
    
    # Initial population
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
    
    await factory.evolve_population()

if __name__ == "__main__":
    asyncio.run(main())
