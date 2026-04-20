# Architecture Notes

This document is for contributors who want to understand *why* things are built the way they are, not just *what* they do.

---

## Data flow: a single request

1. HTTP request arrives at a node's Axum server.
2. The handler extracts the client IP and calls `DecisionEngine::check(ip)`.
3. The engine reads from `RateLimitStore` (shared `Arc<RwLock<HashMap>>`).
4. If the IP entry doesn't exist, it's created with a fresh `SlidingWindow`.
5. `SlidingWindow::estimate()` returns the blended count. If it's under the limit, the request is allowed and the local G-Counter slot is incremented.
6. The response goes back to the client.
7. At the next gossip tick (or immediately, if the node just crossed the threshold), the updated counters are pushed to peers.

---

## The sliding window math

A pure fixed window lets an attacker send `2N` requests in a short burst by sending `N` at the end of one window and `N` at the start of the next. The sliding window approximation fixes this without storing per-request timestamps.

```
elapsed = (now - window_start) / window_duration   // 0.0 to 1.0
estimate = prev_count × (1 - elapsed) + curr_count
```

When `elapsed` reaches 1.0, the window rolls: `prev = curr`, `curr = 0`, `window_start = now`.

The estimate is an approximation. It assumes traffic was uniformly distributed in the previous window, which isn't always true. In practice the error is small enough to be acceptable for most rate limiting scenarios.

---

## Why G-Counter and not something else

Options considered:

- **PN-Counter**: supports decrement, which we don't need. Extra complexity for no gain.
- **LWW register**: last-write-wins would mean a slow node could overwrite a higher count from a faster one. Unsafe for counting.
- **Vector clocks + operational log**: accurate but expensive. Every request would need to be logged and replayed on merge.
- **G-Counter**: monotonic, mergeable with max(), cheap. The only thing you can't do is decrement — which is exactly right for a rate limiter. Counters only go up within a window, and the window itself provides the reset mechanism.

---

## Gossip protocol

### Background gossip

Every second, the node picks `k=3` peers at random from the membership list and initiates a push-pull exchange:

1. **Push**: send a digest of local state (a map of `ip → (node_id, count)` hashes, not full values).
2. **Pull**: peer responds with entries where its version differs from the digest.
3. **Merge**: apply the received entries.

The digest step is important. Without it, every gossip round would ship the entire IP table. With high-cardinality tables (millions of IPs), that's untenable over UDP.

### Adaptive gossip

When `estimate(ip) > threshold × 0.8` (80% of the limit), the node that observed this immediately fans out to `k=5` peers without waiting for the next tick. This reduces propagation latency for the IPs that matter most. The 150ms convergence target assumes a 10-node cluster; larger clusters take longer.

### Membership

Node membership is itself gossiped as regular state, not through a separate protocol. New nodes announce themselves to a seed node; the seed gossips the membership update; within a few rounds every node knows about the new member. This is simpler than running a separate membership service (Serf, etc.) and sufficient for the cluster sizes this is designed for.

---

## Memory model

`RateLimitStore` is a `HashMap<String, SlidingWindow>` wrapped in `Arc<RwLock<_>>`. The `Arc` lets multiple threads hold a reference to the store. The `RwLock` allows concurrent reads (many threads can check rate limits simultaneously) while serializing writes (incrementing a counter, receiving a gossip update, evicting an entry).

Read contention should be minimal. The hot path is: acquire read lock → lookup → release. The only write contention point is gossip receive, which merges a batch of entries under a single write lock. That batch should be applied atomically to avoid a partial merge being visible to readers.

### Eviction

Two mechanisms run together:

**TTL eviction**: any entry not updated in `2 × window_duration` is eligible for removal. This handles IPs that stop sending traffic. Cleanup can be lazy (checked on access) or eager (background sweep). Background sweep is simpler to reason about.

**LRU cap**: when `len() > max_entries`, evict the entry with the oldest `last_seen` timestamp. This bounds memory under DDoS conditions where an attacker rotates IPs rapidly to fill the table.

The combination means normal IPs get cleaned up naturally, and adversarial IPs get evicted under pressure without crashing the process.

---

## Failure modes

**Network partition**: nodes fall back to local counting. The local G-Counter slot for the partitioned node will lag behind the true global count. Traffic that would have been blocked globally may be allowed locally. This is the "fail open" choice. The alternative, blocking traffic when you can't confirm the global count, would mean a network hiccup takes down your rate limiter.

**Node crash**: no state is persisted to disk. When a node restarts, it bootstraps from seed nodes and learns the current state through gossip. There's a short window after restart where the node's local counters are empty and it may allow traffic it would otherwise block. This window shrinks as gossip rounds converge.

**Slow peer**: gossip is fire-and-forget over UDP. A slow peer doesn't block the sending node. If a peer consistently fails to respond, it stays in the membership list until a TTL-based membership eviction removes it (planned feature).

---

## What this is not

- Not a replacement for a hardware load balancer with rate limiting built in (HAProxy, nginx, Cloudflare).
- Not suitable for financial transactions or anything where an undercount has severe consequences.
- Not designed for clusters larger than ~50 nodes without tuning gossip fan-out and interval.
