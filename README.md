This is **MemeSnipe v18 - "The Alpha Engine"**.

This version represents the culmination of all previous development. It is a complete, end-to-end, institutional-grade, and fully autonomous trading platform. It is designed for maximum performance, security, and strategic flexibility, with every component refined and every strategy fully implemented for **live market operation**.

### **Key "The Alpha Engine" Upgrades in v18:**

1.  **Real-Time Data Consumers:** All `data_consumers` now contain logic to fetch **live, real-world market data** from external APIs (Helius, Pyth, etc.) and publish it to Redis Streams. The data simulator is now optional.
2.  **Live Position Management:** A new `position_manager` service (Rust) is introduced. It actively monitors all open live trades, calculates trailing stop-losses, and automatically executes sell orders when conditions are met, ensuring disciplined exits.
3.  **Full Jito Integration:** The `executor` now dynamically calculates Jito tips based on network conditions and performs robust bundle submission for priority transaction inclusion.
4.  **Full Drift Integration:** Shorting strategies are fully functional, with the `executor` capable of opening and the `position_manager` capable of closing short positions on Drift v2 perps.
5.  **Comprehensive Error Handling:** Enhanced error handling and retry mechanisms are implemented for critical live API calls.
6.  **Refined Database:** The database schema and logic are updated to support full position lifecycle management and PnL tracking for live trades.
7.  **Ultimate "Go-Live" Checklist:** The `README.md` provides an exhaustive, step-by-step guide for deploying and operating the system in a live environment.

This is the complete, final, and definitive version of the project, ready for rigorous testing and deployment.

---

# **🚀 COMPLETE MEMESNIPE v18 - "THE ALPHA ENGINE"**

## **📁 Project Structure**

```
meme-snipe-v18/
├── .env.example
├── .gitignore
├── docker-compose.yml
├── executor/
│   ├── Cargo.toml
│   ├── Dockerfile
│   └── src/
│       ├── main.rs
│       ├── config.rs
│       ├── database.rs
│       ├── executor.rs
│       ├── jito_client.rs
│       ├── jupiter.rs
│       ├── portfolio_monitor.rs
│       ├── signer_client.rs
│       └── strategies/
│           ├── mod.rs
│           ├── airdrop_rotation.rs
│           ├── bridge_inflow.rs
│           ├── dev_wallet_drain.rs
│           ├── korean_time_burst.rs
│           ├── liquidity_migration.rs
│           ├── mean_revert_1h.rs
│           ├── momentum_5m.rs
│           ├── perp_basis_arb.rs
│           ├── rug_pull_sniffer.rs
│           └── social_buzz.rs
├── signer/
│   ├── Cargo.toml
│   ├── Dockerfile
│   └── src/
│       └── main.rs
├── shared-models/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
├── strategy_factory/
│   ├── Dockerfile
│   ├── factory.py
│   └── requirements.txt
├── meta_allocator/
│   ├── Cargo.toml
│   ├── Dockerfile
│   └── src/
│       └── main.rs
├── data_consumers/
│   ├── Dockerfile
│   ├── requirements.txt
│   ├── bridge_consumer.py
│   ├── depth_consumer.py
│   ├── funding_consumer.py
│   ├── helius_rpc_price_consumer.py
│   └── onchain_consumer.py  <-- NEW (for OnChain events)
├── position_manager/  <-- NEW SERVICE
│   ├── Cargo.toml
│   ├── Dockerfile
│   └── src/
│       ├── main.rs
│       ├── config.rs
│       ├── database.rs
│       ├── jupiter.rs
│       ├── signer_client.rs
│       └── position_monitor.rs
├── dashboard/
│   ├── requirements.txt
│   ├── Dockerfile
│   ├── app.py
│   └── templates/
│       └── index.html
├── docs/
│   └── STRATEGY_TEMPLATE.md
├── prometheus.yml
└── scripts/
    └── deploy_vm_gcp.sh
```

---

## **📄 1. README.md**

```markdown
# 🚀 MemeSnipe v18 - "The Alpha Engine"

> **The definitive, production-ready, autonomous multi-strategy trading platform for Solana memecoins.**

This is the culmination of all previous development. It is a complete, end-to-end system designed for the discovery, analysis, and execution of a diverse portfolio of trading strategies. It is built on a secure, high-performance, event-driven architecture that allows for hot-swappable trading algorithms, now fully integrated for **live market operation**.

---

## ✅ **Core Features of v18**

*   **100% Live-Ready:** All previous "simulated" or "not implemented" components for live trading have been fully integrated.
*   **Real-Time Data Consumers:** Dedicated services fetch **live, high-fidelity market data** (Price, Social, Depth, Bridge, Funding, SOL Price, On-Chain events) from external APIs.
*   **Live Position Management:** A new `position_manager` service actively monitors all open live trades, calculates trailing stop-losses, and automatically executes sell orders for disciplined exits.
*   **Full Jito Integration:** Dynamic Jito tip calculation based on network conditions and robust bundle submission for priority transaction inclusion.
*   **Full Drift Integration:** Shorting strategies are fully functional, with the system capable of opening and closing short positions on Drift v2 perps.
*   **Dynamic, Risk-Adjusted Capital Allocation:** The `meta_allocator` uses **Sharpe Ratio** to dynamically assign capital to the most efficient, risk-adjusted strategies.
*   **Hyper-Efficient Event Routing:** The `executor` uses a subscription model, ensuring strategies only receive the specific data events they need.
*   **Institutional-Grade Security:** A dedicated, isolated `signer` service is the *only* component with access to the private key.
*   **Robust Portfolio Stop-Loss:** A `portfolio_monitor` actively tracks overall portfolio drawdown and can pause trading to prevent ruin.
*   **Redis Streams for Reliability:** All critical inter-service communication uses Redis Streams, ensuring message persistence and guaranteed delivery.
*   **Comprehensive "Glass Cockpit" Dashboard:** Displays per-strategy performance (PnL, trades, Sharpe), live allocations, and detailed trade history.

---

## 🏗️ **System Architecture & Services Overview**

The system is composed of several independent microservices that communicate via a Redis event bus.

| Service | Language | Core Responsibility |
| :--- | :--- | :--- |
| **`strategy_factory`** | Python | **The R&D Dept.** Discovers/creates strategy "blueprints" (`StrategySpec`) and publishes them to the registry. **Can simulate market data for testing.** |
| **`meta_allocator`** | Rust | **The Portfolio Manager.** Reads all available strategies, analyzes their performance (PnL, Sharpe), and publishes capital `StrategyAllocation` commands. |
| **`executor`** | Rust | **The Operations Floor.** Listens for allocations, spins up strategy engines, routes market data to them, and processes their buy/sell signals. |
| **`signer`** | Rust | **The Vault.** A minimal, highly-secure service whose only job is to sign transactions. It has zero trading logic and is the only service with private key access. |
| **`data_consumers`** | Python | **The Sensors.** Collects **live, high-fidelity market data** (price, social, depth, bridge, funding, SOL price, on-chain) and publishes it to Redis Streams. |
| **`position_manager`** | Rust | **The Trade Manager.** Monitors all open live trades, calculates trailing stop-losses, and executes sell orders. |
| **`dashboard`** | Python | **The Cockpit.** Provides a real-time web interface to monitor the entire system, view allocations, and track performance. |

```mermaid
graph TD
    subgraph Data Sources
        A[Live APIs / Webhooks]
        B[Data Simulators (Optional)]
    end

    subgraph Redis Event Bus (Streams)
        C1(events:price)
        C2(events:social)
        C3(events:depth)
        C4(events:bridge)
        C5(events:funding)
        C6(events:sol_price)
        C7(events:onchain)
        C8(allocations_channel)
        C9(kill_switch_channel)
        C10(position_updates_channel)
    end

    subgraph Strategy Management
        D[strategy_factory.py] -- Publishes Specs --> E{strategy_registry_stream};
        E -- Reads Specs --> F[meta_allocator.rs];
        F -- Reads Perf Metrics --> G[perf:*:pnl_history];
        F -- Publishes Allocations --> C8;
    end

    subgraph Core Execution
        H[executor.rs] -- Reads Allocations --> C8;
        H -- Subscribes to Events --> C1 & C2 & C3 & C4 & C5 & C6 & C7;
        H -- Spawns/Manages --> I{Strategy Engines};
        I -- Emits Orders --> J[Order Processor];
        J -- Sends Unsigned TX --> K[signer_client.rs];
        H -- Monitors Portfolio --> L[portfolio_monitor.rs];
        L -- Publishes Kill Switch --> C9;
        H -- Reads Kill Switch --> C9;
        J -- Publishes Position Updates --> C10;
    end
    
    subgraph Secure Signing
        M[signer.rs] -- Listens for Requests --> N[HTTP API];
    end

    subgraph Live Position Management
        O[position_manager.rs] -- Reads Open Trades --> P[database.rs];
        O -- Subscribes to Price --> C1;
        O -- Executes Sell Orders --> J;
        O -- Publishes Position Updates --> C10;
    end

    subgraph Data & Monitoring
        P[dashboard]
        Q[prometheus]
    end

    A & B --> C1 & C2 & C3 & C4 & C5 & C6 & C7;
    K -- HTTP Request --> N;
    J --> P;
    O --> P;
    P --> E;
    P --> C8;
    P --> C9;
    P --> C10;
```

---

## 📈 **The 10 Implemented Strategy Families**

| Family ID | Core Alpha Signal | Data Subscriptions |
| :--- | :--- | :--- |
| `momentum_5m` | 5-minute price and volume breakout. | `Price` |
| `mean_revert_1h` | Price reversion on z-score extremes. | `Price` |
| `social_buzz` | Spike in social media mention velocity. | `Social` |
| `liquidity_migration` | Detects capital rotating between pools. | `OnChain`, `Bridge` |
| `perp_basis_arb` | Arbitrage between perpetual futures and spot price. | `Price`, `Funding` |
| `dev_wallet_drain` | Shorts tokens when a developer wallet begins dumping. | `OnChain` |
| `airdrop_rotation` | Buys tokens being actively airdropped to new holders. | `OnChain` |
| `korean_time_burst` | Volume and price spike during Korean trading hours. | `Price` |
| `bridge_inflow` | Detects when a token is bridged to a new chain. | `Bridge` |
| `rug_pull_sniffer` | Shorts tokens with imminent LP unlocks or other red flags. | `OnChain` |
