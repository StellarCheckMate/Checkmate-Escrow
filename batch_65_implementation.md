# Storage Migration: Move Per-Address Admin Lists to Persistent Storage

## Issue #1545 Implementation Plan

### Problem
FrozenPlayer, BlacklistedToken, StablecoinIssuer, and AllowedToken are stored in instance storage, which:
- Is loaded on every contract invocation
- Has a size limit that affects all calls
- Causes cost increases as lists grow

### Solution
Move these per-address entries from instance storage to persistent storage with TTL extension.

### Changes Required

#### 1. Update DataKey Enum (types.rs)
Current instance storage keys:
- `FrozenPlayer(Address)` 
- `StablecoinIssuer(Address)`
- `BlacklistedToken(Address)`
- `AllowedToken(Address)`

These remain in the enum but will use persistent storage instead of instance storage.

#### 2. Update Storage Access Patterns (lib.rs)

Replace all occurrences of:
```rust
env.storage().instance().get(&DataKey::FrozenPlayer(addr))
env.storage().instance().set(&DataKey::FrozenPlayer(addr), &value)
env.storage().instance().has(&DataKey::FrozenPlayer(addr))
env.storage().instance().remove(&DataKey::FrozenPlayer(addr))
```

With:
```rust
env.storage().persistent().get(&DataKey::FrozenPlayer(addr))
env.storage().persistent().set(&DataKey::FrozenPlayer(addr), &value)
env.storage().persistent().has(&DataKey::FrozenPlayer(addr))
env.storage().persistent().remove(&DataKey::FrozenPlayer(addr))
```

Same pattern for:
- `BlacklistedToken(Address)`
- `StablecoinIssuer(Address)`
- `AllowedToken(Address)`

#### 3. Add TTL Extension Function

Create helper function to extend TTL for persistent storage entries:
```rust
fn extend_persistent_storage_ttl(env: &Env, lifetime_seconds: u32) {
    env.storage().persistent().extend_ttl(
        PERSISTENT_STORAGE_TTL_THRESHOLD,
        lifetime_seconds,
    );
}
```

Add TTL extension after each persistent storage write for these keys.

#### 4. Migration Step (_apply_migrations)

Add migration to move existing instance storage entries to persistent storage:
```rust
fn migrate_v1_to_v2(env: &Env) {
    // For each entry type, read from instance and write to persistent
    // Remove from instance after successful transfer
}
```

#### 5. Update Documentation

Update `docs/storage-layout.md` to:
- Mark these keys as using persistent storage (with TTL)
- Document TTL extension strategy
- Note when migration occurred (v1 → v2)

### Files to Modify
1. `contracts/escrow/src/lib.rs` - Storage access pattern changes
2. `contracts/escrow/src/types.rs` - If needed for helper structs
3. `docs/storage-layout.md` - Documentation update
4. `contracts/escrow/src/migrations.rs` - Add migration logic

### Constants Needed
```rust
const PERSISTENT_STORAGE_TTL_THRESHOLD: u32 = 17280 * 30; // 30 days
const PERSISTENT_STORAGE_TTL_EXTEND_TO: u32 = 17280 * 60;  // 60 days
```

### Performance Impact
- **Positive**: Reduced instance storage size → lower invocation costs
- **Neutral**: Persistent storage access similar to instance storage performance
- **Action**: TTL extension required regularly to prevent data expiration

### Testing Considerations
- Verify migration doesn't lose data
- Test persistent storage TTL extension works
- Benchmark invocation cost reduction
- Verify FrozenPlayer, BlacklistedToken, StablecoinIssuer, AllowedToken still work correctly
