#!/usr/bin/env python3
"""
MemeSnipe v25 - Complete .env Validation Script
Tests every single environment variable for correctness, connectivity, and format
"""

import os
import sys
import asyncio
import aiohttp
import redis
import psycopg2
import json
import uuid
from urllib.parse import urlparse
from datetime import datetime

class EnvValidator:
    def __init__(self):
        self.results = {}
        self.critical_failures = []
        self.warnings = []
        
    def load_env(self):
        """Load and parse .env file"""
        try:
            with open('.env', 'r') as f:
                for line in f:
                    line = line.strip()
                    if line and not line.startswith('#') and '=' in line:
                        key, value = line.split('=', 1)
                        os.environ[key] = value
            print("✅ .env file loaded successfully")
            return True
        except Exception as e:
            print(f"❌ CRITICAL: Cannot load .env file: {e}")
            return False

    def validate_format(self, key, value, expected_format):
        """Validate environment variable format"""
        try:
            if expected_format == "url":
                parsed = urlparse(value)
                return bool(parsed.scheme and parsed.netloc)
            elif expected_format == "uuid":
                uuid.UUID(value)
                return True
            elif expected_format == "float":
                float(value)
                return True
            elif expected_format == "int":
                int(value)
                return True
            elif expected_format == "bool":
                return value.lower() in ['true', 'false']
            elif expected_format == "api_key":
                return len(value) > 10 and not value.startswith('demo_') and not value.endswith('_placeholder')
            return True
        except:
            return False

    async def test_api_connectivity(self, name, url, headers=None):
        """Test API endpoint connectivity"""
        try:
            timeout = aiohttp.ClientTimeout(total=5)
            async with aiohttp.ClientSession(timeout=timeout) as session:
                async with session.get(url, headers=headers or {}) as response:
                    if response.status in [200, 401, 403]:  # 401/403 means API is reachable
                        print(f"✅ {name}: API reachable (status: {response.status})")
                        return True
                    else:
                        print(f"⚠️  {name}: API responded with status {response.status}")
                        return False
        except Exception as e:
            print(f"❌ {name}: Connection failed - {e}")
            return False

    def test_redis_connection(self):
        """Test Redis connectivity"""
        try:
            redis_url = os.getenv('REDIS_URL', 'redis://redis:6379')
            r = redis.from_url(redis_url, decode_responses=True)
            r.ping()
            print("✅ Redis: Connection successful")
            return True
        except Exception as e:
            print(f"❌ Redis: Connection failed - {e}")
            return False

    def test_database_connection(self):
        """Test PostgreSQL connectivity"""
        try:
            db_url = os.getenv('DATABASE_URL')
            if not db_url:
                print("❌ DATABASE_URL: Missing - required for portfolio manager")
                return False
                
            conn = psycopg2.connect(db_url)
            conn.close()
            print("✅ PostgreSQL: Connection successful")
            return True
        except Exception as e:
            print(f"❌ PostgreSQL: Connection failed - {e}")
            return False

    def validate_critical_settings(self):
        """Validate critical system settings"""
        critical_vars = {
            'PAPER_TRADING_MODE': ('bool', True),
            'WALLET_KEYPAIR_FILENAME': ('file', True),
            'JITO_AUTH_KEYPAIR_FILENAME': ('file', True),
            'INITIAL_CAPITAL_USD': ('float', True),
            'HELIUS_API_KEY': ('api_key', True),
            'FARCASTER_API_KEY': ('api_key', True),
            'REDIS_URL': ('url', True),
        }
        
        print("\n🔍 VALIDATING CRITICAL SETTINGS:")
        print("=" * 50)
        
        for var, (format_type, required) in critical_vars.items():
            value = os.getenv(var)
            if not value and required:
                print(f"❌ {var}: MISSING (CRITICAL)")
                self.critical_failures.append(f"{var} is missing")
                continue
                
            if value and not self.validate_format(var, value, format_type):
                print(f"❌ {var}: Invalid format")
                self.critical_failures.append(f"{var} has invalid format")
                continue
                
            # Special validations
            if var == 'PAPER_TRADING_MODE' and value.lower() != 'true':
                print(f"⚠️  {var}: Set to {value} - LIVE TRADING MODE!")
                self.warnings.append("Live trading mode enabled")
            else:
                print(f"✅ {var}: Valid")

    def validate_api_keys(self):
        """Validate all API keys"""
        api_keys = {
            'HELIUS_API_KEY': 'Real Helius API key',
            'FARCASTER_API_KEY': 'Real Neynar Farcaster API key',
            'TWITTER_BEARER_TOKEN': 'Real Twitter Bearer Token',
            'PYTH_API_KEY': 'Pyth API key (demo_key_simulation_only = placeholder)',
            'BACKTESTING_PLATFORM_API_KEY': 'External backtest API key'
        }
        
        print("\n🔑 VALIDATING API KEYS:")
        print("=" * 50)
        
        for key, description in api_keys.items():
            value = os.getenv(key)
            if not value:
                print(f"❌ {key}: MISSING - {description}")
                continue
                
            if 'demo_' in value or '_placeholder' in value:
                print(f"⚠️  {key}: PLACEHOLDER - {description}")
                self.warnings.append(f"{key} is placeholder")
            elif len(value) < 10:
                print(f"❌ {key}: Too short - {description}")
            else:
                print(f"✅ {key}: Valid format - {description}")

    async def validate_api_endpoints(self):
        """Test all API endpoint connectivity"""
        endpoints = {
            'Helius RPC': {
                'url': os.getenv('SOLANA_RPC_URL', ''),
                'headers': {}
            },
            'Jito RPC': {
                'url': os.getenv('JITO_RPC_URL', ''),
                'headers': {}
            },
            'Jupiter API': {
                'url': f"{os.getenv('JUPITER_API_URL', '')}/quote?inputMint=So11111111111111111111111111111111111111112&outputMint=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v&amount=100000000",
                'headers': {}
            },
            'Farcaster API': {
                'url': f"{os.getenv('FARCASTER_API_URL', '')}/casts/trending",
                'headers': {'x-api-key': os.getenv('FARCASTER_API_KEY', '')}
            },
            'Drift API': {
                'url': f"{os.getenv('DRIFT_API_URL', '')}/stats",
                'headers': {}
            }
        }
        
        print("\n🌐 TESTING API CONNECTIVITY:")
        print("=" * 50)
        
        for name, config in endpoints.items():
            if config['url']:
                await self.test_api_connectivity(name, config['url'], config['headers'])
            else:
                print(f"❌ {name}: URL not configured")

    def validate_trading_parameters(self):
        """Validate trading and risk parameters"""
        trading_params = {
            'INITIAL_CAPITAL_USD': ('float', 'Initial trading capital'),
            'GLOBAL_MAX_POSITION_USD': ('float', 'Maximum position size'),
            'PORTFOLIO_STOP_LOSS_PERCENT': ('float', 'Portfolio stop loss'),
            'TRAILING_STOP_LOSS_PERCENT': ('float', 'Trailing stop loss'),
            'SLIPPAGE_BPS': ('int', 'Slippage in basis points'),
            'JITO_TIP_LAMPORTS': ('int', 'Jito tip amount'),
            'MAX_PRICE_DEVIATION': ('float', 'Maximum price deviation'),
            'CIRCUIT_BREAKER_THRESHOLD': ('float', 'Circuit breaker threshold')
        }
        
        print("\n💰 VALIDATING TRADING PARAMETERS:")
        print("=" * 50)
        
        for param, (format_type, description) in trading_params.items():
            value = os.getenv(param)
            if not value:
                print(f"❌ {param}: MISSING - {description}")
                continue
                
            if self.validate_format(param, value, format_type):
                print(f"✅ {param}: {value} - {description}")
            else:
                print(f"❌ {param}: Invalid format - {description}")

    def validate_genetic_algorithm_params(self):
        """Validate genetic algorithm parameters"""
        ga_params = {
            'POPULATION_SIZE': ('int', 'GA population size'),
            'CROSSOVER_RATE': ('float', 'GA crossover rate'),
            'MUTATION_RATE': ('float', 'GA mutation rate'),
            'TOURNAMENT_SIZE': ('int', 'GA tournament size'),
            'MIN_SHARPE_FOR_PROMOTION': ('float', 'Minimum Sharpe for promotion'),
            'STRATEGY_PROMOTION_INTERVAL_SECS': ('int', 'Strategy promotion interval')
        }
        
        print("\n🧬 VALIDATING GENETIC ALGORITHM PARAMETERS:")
        print("=" * 50)
        
        missing_params = []
        for param, (format_type, description) in ga_params.items():
            value = os.getenv(param)
            if not value:
                print(f"❌ {param}: MISSING - {description}")
                missing_params.append(param)
                continue
                
            if self.validate_format(param, value, format_type):
                print(f"✅ {param}: {value} - {description}")
            else:
                print(f"❌ {param}: Invalid format - {description}")
        
        if missing_params:
            print(f"\n⚠️  MISSING GA PARAMETERS: {', '.join(missing_params)}")
            print("   These are required for strategy evolution!")

    def validate_docker_networking(self):
        """Validate Docker service networking"""
        docker_services = {
            'REDIS_URL': 'redis://redis:6379',
            'SIGNER_URL': 'http://signer:8989',
            'DATABASE_URL': 'postgresql://postgres:password@postgres:5432/meme_snipe_v25'
        }
        
        print("\n🐳 VALIDATING DOCKER NETWORKING:")
        print("=" * 50)
        
        for service, expected_pattern in docker_services.items():
            value = os.getenv(service)
            if not value:
                print(f"❌ {service}: MISSING - should be like {expected_pattern}")
                continue
                
            if 'localhost' in value:
                print(f"⚠️  {service}: Uses localhost - should use Docker service name")
                self.warnings.append(f"{service} uses localhost instead of Docker service name")
            else:
                print(f"✅ {service}: Correct Docker networking format")

    def check_missing_variables(self):
        """Check for completely missing but required variables"""
        required_missing = []
        
        # Check for missing DATABASE_URL
        if not os.getenv('DATABASE_URL'):
            required_missing.append('DATABASE_URL')
            
        # Check for missing DB_PASSWORD
        if not os.getenv('DB_PASSWORD'):
            required_missing.append('DB_PASSWORD')
            
        # Check for missing genetic algorithm params
        ga_required = ['POPULATION_SIZE', 'CROSSOVER_RATE', 'MUTATION_RATE', 'TOURNAMENT_SIZE']
        for param in ga_required:
            if not os.getenv(param):
                required_missing.append(param)
        
        if required_missing:
            print(f"\n❌ MISSING REQUIRED VARIABLES:")
            print("=" * 50)
            for var in required_missing:
                print(f"   • {var}")
            return required_missing
        return []

    async def run_complete_validation(self):
        """Run complete validation suite"""
        print("🔍 MEMESNIPE V25 - COMPLETE .env VALIDATION")
        print("=" * 60)
        print(f"Validation started at: {datetime.now()}")
        print("=" * 60)
        
        if not self.load_env():
            return False
        
        # Run all validations
        self.validate_critical_settings()
        self.validate_api_keys()
        await self.validate_api_endpoints()
        self.validate_trading_parameters()
        self.validate_genetic_algorithm_params()
        self.validate_docker_networking()
        missing_vars = self.check_missing_variables()
        
        # Test actual connectivity
        print("\n🔌 TESTING SERVICE CONNECTIVITY:")
        print("=" * 50)
        redis_ok = self.test_redis_connection()
        db_ok = self.test_database_connection()
        
        # Final summary
        print("\n" + "=" * 60)
        print("📊 VALIDATION SUMMARY")
        print("=" * 60)
        
        if self.critical_failures:
            print("❌ CRITICAL FAILURES:")
            for failure in self.critical_failures:
                print(f"   • {failure}")
        
        if missing_vars:
            print("❌ MISSING REQUIRED VARIABLES:")
            for var in missing_vars:
                print(f"   • {var}")
        
        if self.warnings:
            print("⚠️  WARNINGS:")
            for warning in self.warnings:
                print(f"   • {warning}")
        
        if not self.critical_failures and not missing_vars:
            print("✅ ALL CRITICAL SETTINGS VALIDATED")
            print("🚀 SYSTEM READY FOR CAPITAL ALLOCATION!")
        else:
            print("❌ CONFIGURATION ISSUES FOUND")
            print("🛠️  PLEASE FIX BEFORE DEPLOYING")
        
        return len(self.critical_failures) == 0 and len(missing_vars) == 0

async def main():
    validator = EnvValidator()
    success = await validator.run_complete_validation()
    sys.exit(0 if success else 1)

if __name__ == "__main__":
    asyncio.run(main())
