//! E2E test harness for the Checkmate Escrow + Oracle contracts.
//!
//! Unlike the unit tests in `contracts/*` (which register the contracts
//! natively, compiled into the test binary), these tests deploy the *release
//! WASM artifacts* — the exact bytecode that would be shipped to a live
//! network — into the sandboxed Soroban host and drive the complete match
//! lifecycle through the generated contract clients:
//!
//! create match → deposit ×2 → oracle records the result → escrow settles →
//! players claim the vested payout.

use escrow::EscrowContractClient;
use oracle::OracleContractClient;
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

/// Stake used for every match in these tests.
pub const STAKE: i128 = 100;

/// Amount of the stake token minted to each player.
pub const MINT_AMOUNT: i128 = 1_000;

/// A fully deployed and initialized world: escrow + oracle contracts (both
/// loaded from release WASM), a Stellar asset contract as the stake token,
/// and two funded players.
///
/// The clients carry a (phantom) lifetime from soroban-sdk's generated client
/// types; since the harness never uses the per-client auth-mock builders, the
/// world is constructed with `'static`.
pub struct World<'a> {
    pub env: Env,
    pub escrow: EscrowContractClient<'a>,
    pub oracle: OracleContractClient<'a>,
    pub token: Address,
    pub token_client: TokenClient<'a>,
    /// The oracle service account: admin of the oracle contract and the
    /// address the escrow contract trusts to submit results.
    pub oracle_admin: Address,
    pub admin: Address,
    pub player1: Address,
    pub player2: Address,
}

/// Load a release WASM artifact built by `scripts/test.sh`.
///
/// The tests deliberately run against the compiled bytecode rather than the
/// in-crate Rust code so that what gets validated is what would be deployed.
pub fn load_wasm(name: &str) -> Vec<u8> {
    // wasm32v1-none is the Soroban target (core wasm 1.0 only). Building for
    // wasm32-unknown-unknown on recent Rust emits reference-types /
    // multi-value instructions that the Soroban host validator rejects at
    // deployment — the E2E suite would catch exactly this.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../target/wasm32v1-none/release/{name}.wasm"));
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "E2E tests need the release WASM artifact at {}. Build it first with \
             `cargo build --target wasm32v1-none --release -p escrow -p oracle` \
             (or `scripts/test.sh`). Error: {e}",
            path.display()
        )
    })
}

/// Deploy the escrow and oracle contracts from their release WASM artifacts,
/// deploy a Stellar asset contract, initialize everything, and fund the
/// players.
pub fn deploy_and_fund() -> World<'static> {
    let mut env = Env::default();
    // Don't write test_snapshots/*.json files to disk when the env is dropped.
    env.set_config(soroban_sdk::testutils::EnvTestConfig { capture_snapshot_at_drop: false });
    env.mock_all_auths();

    // Deploy the actual release bytecode — this is what gets shipped.
    let escrow_wasm = load_wasm("escrow");
    let oracle_wasm = load_wasm("oracle");
    let escrow_id = env.register_contract_wasm(None, escrow_wasm.as_slice());
    let oracle_id = env.register_contract_wasm(None, oracle_wasm.as_slice());

    let oracle_admin = Address::generate(&env);
    let admin = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    // USDC-style Stellar asset, minted to both players.
    let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token);
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &MINT_AMOUNT);
    asset_client.mint(&player2, &MINT_AMOUNT);

    // The escrow contract trusts `oracle_admin` — the oracle service account
    // that the off-chain oracle uses to submit results.
    let escrow = EscrowContractClient::new(&env, &escrow_id);
    escrow.initialize(&oracle_admin, &admin);

    // Disable vesting so payouts can be claimed immediately in the test; the
    // default protocol config vests winnings over 3 days.
    escrow.set_protocol_config(&escrow::types::ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: escrow::DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
        minimum_stake: escrow::DEFAULT_MINIMUM_STAKE,
        max_protocol_fee: None,
        dispute_bond_tier_schedule: soroban_sdk::vec![&env],
    });

    let oracle = OracleContractClient::new(&env, &oracle_id);
    oracle.initialize(&oracle_admin);

    World { env, escrow, oracle, token, token_client, oracle_admin, admin, player1, player2 }
}
