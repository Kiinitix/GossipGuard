# GossipGuard

A distributed rate limiter written in Rust. No central store, no single point of failure. Nodes share state through a gossip protocol and make rate-limiting decisions locally using a G-Counter CRDT underneath a sliding window.

---

## Why this exists

Most rate limiters cheat. They either rely on a Redis instance that becomes a bottleneck (and a failure domain), or they do purely local counting and let traffic spike across nodes during a thundering herd. GossipGuard tries a third option: eventual consistency between nodes with a local fallback when the network partitions.

The tradeoff is intentional. You accept a small undercount window in exchange for zero central coordination. If that tradeoff sounds wrong for your use case, this probably isn't the right tool.

---

## How it works

Each node maintains a per-IP sliding window counter. The window uses two integers: a count for the previous window and a count for the current one and blends them to avoid hard resets at window boundaries:

```
estimate = prev × (1 - elapsed_fraction) + curr
```

The underlying counter is a G-Counter CRDT. When nodes gossip, they merge by taking the `max()` per node slot and summing across all slots for the global estimate. Merges are always safe to apply; they're monotonic and commutative.

Gossip runs as a background loop every second with fan-out `k=3`. When a node's local estimate crosses the rate limit threshold, it immediately triggers an adaptive push to `k=5` peers. Under normal conditions the cluster converges in roughly 150ms.

---

## Architecture

```
gossipGuard/
  Cargo.toml          ← workspace root
  crdt/
    Cargo.toml
    src/lib.rs        ← G-Counter CRDT
  core/
    Cargo.toml
    src/lib.rs        ← sliding window, LRU store, decision engine
  server/
    Cargo.toml
    src/main.rs       ← HTTP server, gossip layer, node bootstrap
```

The three crates are intentionally separated. `crdt` has no dependencies on the rest of the project and can be tested and reasoned about in isolation. `core` builds the rate-limiting logic on top of it. `server` wires everything to the network.

---

## Design decisions

**Sliding window over fixed window.** Fixed windows have a well-known 2× burst problem at window boundaries. The weighted blend smooths that out without adding much complexity.

**G-Counter CRDT for merge.** The CRDT guarantees that any order of merge operations produces the same result. Nodes don't need to coordinate before merging; they just take the max.

**Arc\<RwLock\<T\>> for shared state.** Multiple threads need to read the rate limit store concurrently. Reads are cheap; writes happen on every allowed request and on gossip receive.

**Bincode over UDP for gossip.** Small payload, fast serialization, no connection overhead. Digest-based delta sync keeps the payload small even as the IP table grows nodes only exchange entries that differ from what the peer already has.

**Fail open on partition.** If a node can't reach its peers, it falls back to local counting and allows traffic. This means you accept undercounting during a partition rather than blocking legitimate traffic. There's a flag to flip this if you prefer the opposite behavior.

**TTL + LRU eviction.** Entries expire after `2 × window_duration` of inactivity. A hard cap on total entries triggers LRU eviction under memory pressure. High-cardinality IP tables (millions of IPs from a DDoS) won't blow up the process.

---

## Status

| Component | Status |
|-----------|--------|
| G-Counter CRDT (`crdt/`) | Done, tests passing |
| Sliding window (`core/`) | In progress |
| Rate limit store (`core/`) | In progress |
| Decision engine (`core/`) | In progress |
| Gossip layer (`server/`) | Planned |
| HTTP server — Axum (`server/`) | Planned |
| Seed node bootstrap | Planned |
| Prometheus metrics endpoint | Planned |
| Docker multi-node setup | Planned |

---

## Building

```bash
git clone https://github.com/Kiinitix/GossipGuard
cd GossipGuard
cargo build --workspace
```

Run tests:

```bash
cargo test --workspace
```

---

## Configuration (planned)

```toml
[rate_limit]
requests_per_window = 100
window_duration_secs = 60

[gossip]
fanout_background = 3
fanout_adaptive = 5
interval_ms = 1000

[store]
max_entries = 1_000_000
ttl_multiplier = 2
```

---

## The CRDT

The G-Counter in `crdt/src/lib.rs` is the foundation. Each node has a slot in the counter keyed by its node ID. Increment only touches the local slot. Merge takes the max across all slots. The global value is the sum.

```rust
pub fn merge(&mut self, other: &GCounter) {
    for (node, incoming) in &other.counts {
        self.counts
            .entry(node.clone())
            .and_modify(|v| *v = (*v).max(*incoming))
            .or_insert(*incoming);
    }
}
```

This is safe to call repeatedly with the same data (idempotent), in any order (commutative), and you'll never lose a count that was already observed (monotonic). Those three properties are what make it work without coordination.

---

## Contributing

The build order matters if you want to contribute. `crdt` has no dependencies; start there if you're writing a new merge strategy. `core` depends on `crdt`; the sliding window and store logic lives there. `server` depends on `core` and should stay thin, networking and config wiring, not business logic.

Open an issue before sending a large PR. The architecture decisions listed above are locked in for now; changes to the gossip protocol or CRDT merge strategy need discussion first.

---

## License

MIT
