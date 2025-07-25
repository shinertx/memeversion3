import asyncio
import json
import os
import random
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

    async def submit_for_backtest(self, genome: StrategyGenome) -> Optional[str]:
        """Submit a strategy to the external backtesting platform."""
        spec = self.genome_to_spec(genome)
        
        try:
            response = await self.http_client.post(
                f"{BACKTESTING_API_URL}/backtest",
                json={
                    "strategy_spec": asdict(spec),
                    "lookback_days": 30,
                    "initial_capital": 10000.0
                }
            )
            
            if response.status_code == 200:
                result = response.json()
                job_id = result.get("job_id")
                logger.info(f"Submitted strategy {spec.id} for backtest, job_id: {job_id}")
                
                # Store the job_id for the portfolio manager to track
                await self.redis_client.xadd(
                    "backtest_jobs_submitted",
                    {"job_id": job_id, "strategy_id": spec.id, "spec": json.dumps(asdict(spec))}
                )
                
                return job_id
            else:
                logger.error(f"Backtest submission failed: {response.status_code} - {response.text}")
                return None
                
        except Exception as e:
            logger.error(f"Error submitting backtest: {e}")
            return None

    async def evolve_population(self):
        """Main evolution loop."""
        while True:
            logger.info(f"Generation {self.generation}: Evolving {len(self.population)} strategies")
            
            # Create offspring
            offspring = []
            while len(offspring) < POPULATION_SIZE:
                if random.random() < CROSSOVER_RATE:
                    parent1 = self.tournament_select()
                    parent2 = self.tournament_select()
                    child1, child2 = self.crossover(parent1, parent2)
                    offspring.extend([child1, child2])
                else:
                    parent = self.tournament_select()
                    offspring.append(self.mutate(parent))
            
            # Submit new strategies for backtesting
            for genome in offspring[:10]:  # Limit to avoid overwhelming the API
                await self.submit_for_backtest(genome)
            
            # Replace worst performers with offspring
            self.population.sort(key=lambda g: g.fitness)
            num_to_replace = int(len(self.population) * (1 - TOP_SURVIVORS_RATIO))
            self.population[:num_to_replace] = offspring[:num_to_replace]
            
            # Push best strategies to Redis for live simulation
            best_strategies = self.population[-5:]  # Top 5
            for genome in best_strategies:
                spec = self.genome_to_spec(genome)
                await self.redis_client.xadd(
                    "strategy_specs",
                    {"spec": json.dumps(asdict(spec))}
                )
            
            self.generation += 1
            await asyncio.sleep(300)  # 5 minutes between generations

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
            await factory.redis_client.xadd(
                "strategy_specs",
                {"spec": json.dumps(asdict(genome))}
            )
            logger.info(f"Proposed initial strategy: {genome.id}")
    
    await factory.evolve_population()

if __name__ == "__main__":
    asyncio.run(main())
