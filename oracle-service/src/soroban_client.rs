//! Soroban RPC client for building and submitting `submit_result` transactions.
//!
//! ## Architecture
//!
//! We interact with the Soroban RPC's JSON-RPC 2.0 interface directly over
//! HTTPS rather than shelling out to the Stellar CLI.  This lets the oracle
//! run in a minimal container image with no additional tooling.
//!
//! ### Transaction flow
//!
//! 1. **`getAccount`** — fetch the current sequence number for the oracle key.
//! 2. Build an [`InvokeHostFunctionOp`] that calls `submit_result(match_id,
//!    winner)` on the escrow contract.
//! 3. **`simulateTransaction`** — obtain the resource fee estimate and the
//!    soroban auth footprint that must be included in the final transaction.
//! 4. Attach the footprint, compute fees, sign the transaction envelope with
//!    the oracle ed25519 key.
//! 5. **`sendTransaction`** — submit the XDR-encoded signed envelope.
//! 6. **`getTransaction`** — poll until the transaction is confirmed or
//!    failed (Stellar's finality is ~5-6 seconds in practice).

use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use stellar_xdr::{
    AccountId, ContractId, Hash, InvokeContractArgs, InvokeHostFunctionOp, Limits, Memo,
    MuxedAccount, Operation, OperationBody, Preconditions, PublicKey, ScAddress, ScSymbol, ScVal,
    SequenceNumber, SorobanAuthorizationEntry, SorobanResources, SorobanTransactionData,
    SorobanTransactionDataExt, Transaction, TransactionEnvelope, TransactionExt,
    TransactionV1Envelope, Uint256, VecM, WriteXdr,
};
use zeroize::Zeroizing;

use crate::config::Platform;
use crate::oracle::errors::OracleServiceError;

/// A minimal projection of the escrow contract's `Match` struct, decoded from
/// the return value of `get_active_matches_paginated`. Only the fields the
/// reconciliation task needs to enqueue a match for verification are
/// extracted; the rest of the on-chain `Match` record is ignored.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveMatchSummary {
    pub match_id: u64,
    pub game_id: String,
    pub platform: Platform,
}

/// A thin wrapper around the Soroban RPC endpoint.
#[derive(Clone)]
pub struct SorobanClient {
    http: reqwest::Client,
    rpc_url: String,
    network_passphrase: String,
    contract_escrow: [u8; 32],
    _oracle_address: AccountId,
}

impl SorobanClient {
    /// Construct a client from config values.
    ///
    /// * `rpc_url` — e.g. `https://soroban-testnet.stellar.org`
    /// * `network_passphrase` — e.g. `"Test SDF Network ; September 2015"`
    /// * `contract_escrow` — strkey C-address of the escrow contract
    pub fn new(
        rpc_url: String,
        network_passphrase: String,
        contract_escrow_strkey: &str,
    ) -> Result<Self, OracleServiceError> {
        // Fail fast if we're in production and someone has misconfigured
        // `STELLAR_RPC_URL` to a plain HTTP endpoint — submitting oracle
        // results over HTTP would allow an on-path attacker to MITM
        // submissions.
        let production_mode = std::env::var("ORACLE_ENV")
            .map(|v| v.eq_ignore_ascii_case("production"))
            .unwrap_or(false);
        if production_mode && !rpc_url.starts_with("https://") {
            return Err(OracleServiceError::Config(format!(
                "STELLAR_RPC_URL must use HTTPS in production mode, got: {rpc_url}"
            )));
        }

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            // Belt-and-suspenders: refuse to send any request over plain
            // HTTP at the transport layer too, regardless of mode.
            .https_only(true)
            .build()
            .map_err(|e| OracleServiceError::Transport(e.to_string()))?;

        let contract_escrow = decode_contract_id(contract_escrow_strkey)?;
        let oracle_address = AccountId(PublicKey::PublicKeyTypeEd25519(Uint256([0u8; 32]))); // placeholder; set per-call

        Ok(Self {
            http,
            rpc_url,
            network_passphrase,
            contract_escrow,
            _oracle_address: oracle_address,
        })
    }

    /// Sign and submit a `submit_result(match_id, winner)` call to the escrow
    /// contract.
    ///
    /// Returns the transaction hash on success.
    pub async fn submit_result(
        &self,
        match_id: u64,
        winner: &crate::oracle::Winner,
        signing_key: &Zeroizing<[u8; 32]>,
    ) -> Result<String, OracleServiceError> {
        let signing_key_obj = SigningKey::from_bytes(signing_key);
        let pubkey_bytes: [u8; 32] = signing_key_obj.verifying_key().to_bytes();

        // ── 1. Fetch account sequence number ─────────────────────────────
        let g_address = pubkey_to_g_address(&pubkey_bytes)?;
        let sequence = self.get_account_sequence(&g_address).await?;

        // ── 2. Build the InvokeHostFunction operation ─────────────────────
        let (op, _winner_val) =
            build_invoke_op(&self.contract_escrow, match_id, winner, &pubkey_bytes)?;

        // ── 3. Build a preliminary transaction (no resource fees yet) ──────
        let source = MuxedAccount::Ed25519(Uint256(pubkey_bytes));
        let tx = build_transaction(source.clone(), sequence + 1, op.clone(), 100)?;

        // ── 4. simulateTransaction to get fee + auth ──────────────────────
        let sim = self.simulate_transaction(&tx).await?;

        // ── 5. Re-build with correct fees and soroban data ────────────────
        let min_fee: i64 = sim.min_resource_fee.parse().unwrap_or(0);
        let total_fee = (100_000i64 + min_fee) as u32; // base + resource fee

        let soroban_data = decode_soroban_data(&sim.transaction_data)?;
        let auth_entries = decode_auth_entries(&sim.results)?;

        let op_with_auth = build_invoke_op_with_auth(
            &self.contract_escrow,
            match_id,
            winner,
            &pubkey_bytes,
            auth_entries,
        )?;

        let final_tx = build_transaction_with_soroban(
            source.clone(),
            sequence + 1,
            op_with_auth,
            total_fee,
            soroban_data,
        )?;

        // ── 6. Sign ───────────────────────────────────────────────────────
        let envelope = sign_transaction(final_tx, &self.network_passphrase, &signing_key_obj)?;

        // ── 7. sendTransaction ────────────────────────────────────────────
        let tx_hash = self.send_transaction(&envelope).await?;

        // ── 8. Wait for confirmation ──────────────────────────────────────
        self.await_confirmation(&tx_hash).await?;

        Ok(tx_hash)
    }

    /// Fetch one page of `Active` matches from the escrow contract via
    /// `get_active_matches_paginated`.
    ///
    /// `source_pubkey` is only used as the transaction's source account for
    /// simulation — this is a read-only call, nothing is signed or submitted.
    pub async fn get_active_matches_paginated(
        &self,
        offset: u32,
        limit: u32,
        source_pubkey: &[u8; 32],
    ) -> Result<Vec<ActiveMatchSummary>, OracleServiceError> {
        let args = vec![ScVal::U32(offset), ScVal::U32(limit)];
        let result = self
            .simulate_read_call(
                &self.contract_escrow,
                "get_active_matches_paginated",
                args,
                source_pubkey,
            )
            .await?;
        decode_active_matches(&result)
    }

    /// Ask the oracle contract whether a result has already been recorded for
    /// `match_id` via `has_result`.
    pub async fn has_result(
        &self,
        contract_oracle_strkey: &str,
        match_id: u64,
        source_pubkey: &[u8; 32],
    ) -> Result<bool, OracleServiceError> {
        let contract_oracle = decode_contract_id(contract_oracle_strkey)?;
        let args = vec![ScVal::U64(match_id)];
        let result = self
            .simulate_read_call(&contract_oracle, "has_result", args, source_pubkey)
            .await?;
        match result {
            ScVal::Bool(b) => Ok(b),
            other => Err(OracleServiceError::RpcError(format!(
                "has_result: unexpected return value type: {:?}",
                other
            ))),
        }
    }

    /// Simulate a read-only contract call and return its decoded ScVal
    /// result. Never signs or submits anything on-chain.
    async fn simulate_read_call(
        &self,
        contract_id: &[u8; 32],
        function_name: &str,
        args: Vec<ScVal>,
        source_pubkey: &[u8; 32],
    ) -> Result<ScVal, OracleServiceError> {
        let g_address = pubkey_to_g_address(source_pubkey)?;
        let sequence = self.get_account_sequence(&g_address).await?;

        let contract_address = ScAddress::Contract(ContractId(Hash(*contract_id)));
        let args_vecm: VecM<ScVal> = args
            .try_into()
            .map_err(|e| OracleServiceError::XdrError(format!("args vec: {:?}", e)))?;
        let invoke_args = InvokeContractArgs {
            contract_address,
            function_name: ScSymbol(
                function_name
                    .try_into()
                    .map_err(|e| OracleServiceError::XdrError(format!("fn name: {:?}", e)))?,
            ),
            args: args_vecm,
        };
        let op = Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: stellar_xdr::HostFunction::InvokeContract(invoke_args),
                auth: VecM::default(),
            }),
        };

        let source = MuxedAccount::Ed25519(Uint256(*source_pubkey));
        let tx = build_transaction(source, sequence + 1, op, 100)?;
        let sim = self.simulate_transaction(&tx).await?;

        let xdr_str = sim
            .results
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r.get("xdr"))
            .and_then(|x| x.as_str())
            .ok_or_else(|| {
                OracleServiceError::RpcError(format!(
                    "{}: missing return value in simulation results",
                    function_name
                ))
            })?;

        use stellar_xdr::ReadXdr;
        ScVal::from_xdr_base64(xdr_str, Limits::none())
            .map_err(|e| OracleServiceError::XdrError(format!("decode return value: {}", e)))
    }

    // ── RPC helpers ───────────────────────────────────────────────────────────

    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value, OracleServiceError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| OracleServiceError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(OracleServiceError::RpcError(format!(
                "HTTP {} from RPC",
                resp.status()
            )));
        }

        let json: Value = resp
            .json()
            .await
            .map_err(|e| OracleServiceError::Transport(e.to_string()))?;

        if let Some(err) = json.get("error") {
            return Err(OracleServiceError::RpcError(err.to_string()));
        }

        Ok(json["result"].clone())
    }

    async fn get_account_sequence(&self, address: &str) -> Result<i64, OracleServiceError> {
        let result = self
            .rpc_call("getAccount", json!({ "address": address }))
            .await?;

        let seq = result["sequence"]
            .as_str()
            .or_else(|| result["sequenceNumber"].as_str())
            .ok_or_else(|| OracleServiceError::RpcError("missing sequence in getAccount".into()))?;

        seq.parse::<i64>()
            .map_err(|e| OracleServiceError::RpcError(format!("bad sequence: {}", e)))
    }

    async fn simulate_transaction(
        &self,
        tx: &Transaction,
    ) -> Result<SimulateResult, OracleServiceError> {
        let xdr = tx
            .to_xdr_base64(Limits::none())
            .map_err(|e| OracleServiceError::XdrError(e.to_string()))?;

        // simulateTransaction accepts either a raw transaction or a
        // TransactionEnvelope; use a FeeBumpTransaction wrapper format.
        let result = self
            .rpc_call("simulateTransaction", json!({ "transaction": xdr }))
            .await?;

        let error = result.get("error").and_then(|v| v.as_str());
        if let Some(e) = error {
            return Err(OracleServiceError::SimulateError(e.to_string()));
        }

        let min_resource_fee = result["minResourceFee"].as_str().unwrap_or("0").to_string();
        let transaction_data = result["transactionData"].as_str().unwrap_or("").to_string();

        let results = result["results"].clone();

        Ok(SimulateResult {
            min_resource_fee,
            transaction_data,
            results,
        })
    }

    async fn send_transaction(
        &self,
        envelope: &TransactionEnvelope,
    ) -> Result<String, OracleServiceError> {
        let xdr = envelope
            .to_xdr_base64(Limits::none())
            .map_err(|e| OracleServiceError::XdrError(e.to_string()))?;

        let result = self
            .rpc_call("sendTransaction", json!({ "transaction": xdr }))
            .await?;

        let status = result["status"].as_str().unwrap_or("UNKNOWN");
        if status == "ERROR" {
            let detail = result["errorResultXdr"].as_str().unwrap_or("");
            return Err(OracleServiceError::SendError(format!(
                "sendTransaction ERROR: {}",
                detail
            )));
        }

        let hash = result["hash"].as_str().ok_or_else(|| {
            OracleServiceError::RpcError("missing hash in sendTransaction".into())
        })?;

        Ok(hash.to_string())
    }

    /// Poll `getTransaction` until it reaches a terminal state.
    async fn await_confirmation(&self, tx_hash: &str) -> Result<(), OracleServiceError> {
        let max_polls = 30u32;
        let poll_delay = Duration::from_secs(2);

        for _ in 0..max_polls {
            tokio::time::sleep(poll_delay).await;

            let result = self
                .rpc_call("getTransaction", json!({ "hash": tx_hash }))
                .await?;

            let status = result["status"].as_str().unwrap_or("NOT_FOUND");
            match status {
                "SUCCESS" => return Ok(()),
                "FAILED" => {
                    let detail = result["resultXdr"].as_str().unwrap_or("(no detail)");
                    return Err(OracleServiceError::TxFailed(format!(
                        "transaction {} failed: {}",
                        tx_hash, detail
                    )));
                }
                _ => {
                    // "NOT_FOUND" or "PENDING" — keep polling
                }
            }
        }

        Err(OracleServiceError::TxFailed(format!(
            "transaction {} did not confirm within {}s",
            tx_hash,
            max_polls * poll_delay.as_secs() as u32
        )))
    }

    /// Health check: test Stellar RPC connectivity.
    pub async fn health_check(&self) -> Result<(), OracleServiceError> {
        self.rpc_call("getNetwork", json!({})).await.map(|_| ())
    }

    /// Health check: test contract reachability by attempting a read.
    pub async fn contract_health_check(&self, contract_id: &str) -> Result<(), OracleServiceError> {
        let contract_bytes = decode_contract_id(contract_id)?;
        // Attempt to fetch contract data — this will fail if the contract doesn't exist
        // but will succeed if RPC is reachable and the contract is deployed.
        let contract_id_xdr = stellar_xdr::ContractId(stellar_xdr::Hash(contract_bytes));
        let address = stellar_xdr::ScAddress::Contract(contract_id_xdr);

        let address_xdr = address
            .to_xdr_base64(stellar_xdr::Limits::none())
            .map_err(|e| OracleServiceError::XdrError(e.to_string()))?;

        self.rpc_call(
            "getLedgerEntries",
            json!({ "keys": [format!("AAAAAAxQAAAAAA=={}", address_xdr)] }),
        )
        .await
        .map(|_| ())
    }
}

// ── Types ─────────────────────────────────────────────────────────────────────

struct SimulateResult {
    min_resource_fee: String,
    transaction_data: String,
    results: Value,
}

// ── XDR construction helpers ──────────────────────────────────────────────────

/// Convert a winner variant to the ScVal that the escrow contract expects.
fn winner_to_sc_val(winner: &crate::oracle::Winner) -> ScVal {
    let sym = match winner {
        crate::oracle::Winner::Player1 => "Player1",
        crate::oracle::Winner::Player2 => "Player2",
        crate::oracle::Winner::Draw => "Draw",
    };
    // Enums on-chain are represented as ScVal::Symbol
    ScVal::Symbol(ScSymbol(sym.try_into().expect("short symbol")))
}

// ── Read-call return-value decoding ─────────────────────────────────────────
//
// `#[contracttype]` structs are encoded as `ScVal::Map(Some(ScMap))` with one
// entry per field, keyed by a `Symbol` matching the Rust field name. Fieldless
// enum variants (e.g. `Platform::Lichess`) are encoded as
// `ScVal::Vec(Some(vec![ScVal::Symbol(variant_name)]))`. See
// `soroban-sdk-macros`' `derive_struct`/`derive_enum` for the canonical
// encoding this mirrors.

/// Look up a field by name in a decoded struct `ScMap`.
fn map_get<'a>(map: &'a [stellar_xdr::ScMapEntry], key: &str) -> Option<&'a ScVal> {
    map.iter()
        .find(|entry| matches!(&entry.key, ScVal::Symbol(s) if s.0.to_utf8_string().as_deref() == Ok(key)))
        .map(|entry| &entry.val)
}

fn decode_platform(val: &ScVal) -> Result<Platform, OracleServiceError> {
    let ScVal::Vec(Some(vec)) = val else {
        return Err(OracleServiceError::XdrError(format!(
            "platform: expected Vec-encoded enum, got {:?}",
            val
        )));
    };
    match vec.0.first() {
        Some(ScVal::Symbol(sym)) => {
            let name = sym.0.to_utf8_string().map_err(|e| {
                OracleServiceError::XdrError(format!("platform: symbol decode: {}", e))
            })?;
            match name.as_str() {
                "Lichess" => Ok(Platform::Lichess),
                "ChessDotCom" => Ok(Platform::ChessDotCom),
                other => Err(OracleServiceError::XdrError(format!(
                    "platform: unknown variant '{}'",
                    other
                ))),
            }
        }
        other => Err(OracleServiceError::XdrError(format!(
            "platform: expected leading Symbol discriminant, got {:?}",
            other
        ))),
    }
}

fn decode_match_summary(val: &ScVal) -> Result<ActiveMatchSummary, OracleServiceError> {
    let ScVal::Map(Some(map)) = val else {
        return Err(OracleServiceError::XdrError(format!(
            "Match: expected Map-encoded struct, got {:?}",
            val
        )));
    };

    let match_id = match map_get(&map.0, "id") {
        Some(ScVal::U64(id)) => *id,
        other => {
            return Err(OracleServiceError::XdrError(format!(
                "Match.id: expected U64, got {:?}",
                other
            )))
        }
    };

    let game_id = match map_get(&map.0, "game_id") {
        Some(ScVal::String(s)) => s.0.to_utf8_string().map_err(|e| {
            OracleServiceError::XdrError(format!("Match.game_id: string decode: {}", e))
        })?,
        other => {
            return Err(OracleServiceError::XdrError(format!(
                "Match.game_id: expected String, got {:?}",
                other
            )))
        }
    };

    let platform_val = map_get(&map.0, "platform")
        .ok_or_else(|| OracleServiceError::XdrError("Match.platform: field missing".to_string()))?;
    let platform = decode_platform(platform_val)?;

    Ok(ActiveMatchSummary {
        match_id,
        game_id,
        platform,
    })
}

fn decode_active_matches(val: &ScVal) -> Result<Vec<ActiveMatchSummary>, OracleServiceError> {
    let ScVal::Vec(Some(vec)) = val else {
        return Err(OracleServiceError::XdrError(format!(
            "get_active_matches_paginated: expected Vec return value, got {:?}",
            val
        )));
    };
    vec.0.iter().map(decode_match_summary).collect()
}

fn decode_contract_id(strkey: &str) -> Result<[u8; 32], OracleServiceError> {
    use stellar_strkey::Contract;
    let contract = Contract::from_string(strkey).map_err(|e| {
        OracleServiceError::Config(format!("invalid contract strkey '{}': {}", strkey, e))
    })?;
    Ok(contract.0)
}

fn pubkey_to_g_address(pubkey: &[u8; 32]) -> Result<String, OracleServiceError> {
    Ok(format!("{}", stellar_strkey::ed25519::PublicKey(*pubkey)))
}

fn build_invoke_op(
    contract_id: &[u8; 32],
    match_id: u64,
    winner: &crate::oracle::Winner,
    _caller_pubkey: &[u8; 32],
) -> Result<(Operation, ScVal), OracleServiceError> {
    build_invoke_op_with_auth(contract_id, match_id, winner, _caller_pubkey, vec![])
        .map(|op| (op, winner_to_sc_val(winner)))
}

fn build_invoke_op_with_auth(
    contract_id: &[u8; 32],
    match_id: u64,
    winner: &crate::oracle::Winner,
    caller_pubkey: &[u8; 32],
    auth_entries: Vec<SorobanAuthorizationEntry>,
) -> Result<Operation, OracleServiceError> {
    let contract_address = ScAddress::Contract(ContractId(Hash(*contract_id)));

    // submit_result(match_id: u64, winner: Winner, caller: Address)
    // caller is implicit via auth; pass match_id and winner as args.
    let match_id_val = ScVal::U64(match_id);
    let winner_val = winner_to_sc_val(winner);

    // Caller address as ScVal::Address
    let caller_address = ScVal::Address(ScAddress::Account(AccountId(
        PublicKey::PublicKeyTypeEd25519(Uint256(*caller_pubkey)),
    )));

    let args: VecM<ScVal> = vec![match_id_val, winner_val, caller_address]
        .try_into()
        .map_err(|e| OracleServiceError::XdrError(format!("args vec: {:?}", e)))?;

    let invoke_args = InvokeContractArgs {
        contract_address,
        function_name: ScSymbol(
            "submit_result"
                .try_into()
                .map_err(|e| OracleServiceError::XdrError(format!("fn name: {:?}", e)))?,
        ),
        args,
    };

    let auth_vec: VecM<SorobanAuthorizationEntry> = auth_entries
        .try_into()
        .map_err(|e| OracleServiceError::XdrError(format!("auth vec: {:?}", e)))?;

    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
            host_function: stellar_xdr::HostFunction::InvokeContract(invoke_args),
            auth: auth_vec,
        }),
    };

    Ok(op)
}

fn build_transaction(
    source: MuxedAccount,
    sequence: i64,
    op: Operation,
    fee: u32,
) -> Result<Transaction, OracleServiceError> {
    let ops: VecM<Operation, 100> = vec![op]
        .try_into()
        .map_err(|e| OracleServiceError::XdrError(format!("ops vec: {:?}", e)))?;

    Ok(Transaction {
        source_account: source,
        fee,
        seq_num: SequenceNumber(sequence),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: ops,
        ext: TransactionExt::V0,
    })
}

fn build_transaction_with_soroban(
    source: MuxedAccount,
    sequence: i64,
    op: Operation,
    fee: u32,
    soroban_data: SorobanTransactionData,
) -> Result<Transaction, OracleServiceError> {
    let ops: VecM<Operation, 100> = vec![op]
        .try_into()
        .map_err(|e| OracleServiceError::XdrError(format!("ops vec: {:?}", e)))?;

    Ok(Transaction {
        source_account: source,
        fee,
        seq_num: SequenceNumber(sequence),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: ops,
        ext: TransactionExt::V1(soroban_data),
    })
}

fn decode_soroban_data(base64_xdr: &str) -> Result<SorobanTransactionData, OracleServiceError> {
    if base64_xdr.is_empty() {
        // Return empty soroban data
        return Ok(SorobanTransactionData {
            ext: SorobanTransactionDataExt::V0,
            resources: SorobanResources {
                footprint: stellar_xdr::LedgerFootprint {
                    read_only: VecM::default(),
                    read_write: VecM::default(),
                },
                instructions: 0,
                disk_read_bytes: 0,
                write_bytes: 0,
            },
            resource_fee: 0,
        });
    }
    use stellar_xdr::ReadXdr;
    SorobanTransactionData::from_xdr_base64(base64_xdr, Limits::none())
        .map_err(|e| OracleServiceError::XdrError(format!("soroban data decode: {}", e)))
}

fn decode_auth_entries(
    results: &Value,
) -> Result<Vec<SorobanAuthorizationEntry>, OracleServiceError> {
    use stellar_xdr::ReadXdr;
    let arr = match results.as_array() {
        Some(a) => a,
        None => return Ok(vec![]),
    };

    let mut entries = Vec::new();
    for result in arr {
        if let Some(auth_arr) = result.get("auth").and_then(|a| a.as_array()) {
            for auth_xdr in auth_arr {
                if let Some(xdr_str) = auth_xdr.as_str() {
                    let entry = SorobanAuthorizationEntry::from_xdr_base64(xdr_str, Limits::none())
                        .map_err(|e| OracleServiceError::XdrError(format!("auth entry: {}", e)))?;
                    entries.push(entry);
                }
            }
        }
    }
    Ok(entries)
}

/// Sign a transaction and return the V1 envelope.
fn sign_transaction(
    tx: Transaction,
    network_passphrase: &str,
    signing_key: &SigningKey,
) -> Result<TransactionEnvelope, OracleServiceError> {
    use sha2::{Digest, Sha256};
    use stellar_xdr::{DecoratedSignature, Signature, SignatureHint, WriteXdr};

    // Compute the transaction hash = SHA-256(network_id || ENVELOPE_TYPE_TX || tx_xdr)
    let network_id: [u8; 32] = Sha256::digest(network_passphrase.as_bytes()).into();

    let tx_xdr = tx
        .to_xdr(Limits::none())
        .map_err(|e| OracleServiceError::XdrError(e.to_string()))?;

    let mut payload = Vec::with_capacity(4 + 32 + tx_xdr.len());
    // ENVELOPE_TYPE_TX = 2 as u32 big-endian
    payload.extend_from_slice(&2u32.to_be_bytes());
    payload.extend_from_slice(&network_id);
    payload.extend_from_slice(&tx_xdr);

    let hash: [u8; 32] = Sha256::digest(&payload).into();

    let sig = signing_key.sign(&hash);
    let sig_bytes: [u8; 64] = sig.to_bytes();

    let hint = {
        let vk = signing_key.verifying_key().to_bytes();
        let mut h = [0u8; 4];
        h.copy_from_slice(&vk[28..32]);
        SignatureHint(h)
    };

    let decorated = DecoratedSignature {
        hint,
        signature: Signature(
            sig_bytes[..]
                .try_into()
                .map_err(|e| OracleServiceError::XdrError(format!("sig bytes: {:?}", e)))?,
        ),
    };

    let signatures: VecM<DecoratedSignature, 20> = vec![decorated]
        .try_into()
        .map_err(|e| OracleServiceError::XdrError(format!("sigs vec: {:?}", e)))?;

    Ok(TransactionEnvelope::Tx(TransactionV1Envelope {
        tx,
        signatures,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::{ScMap, ScMapEntry, ScString, ScVec};

    /// Build a `ScVal` mirroring how `#[contracttype]` encodes a fieldless
    /// enum variant: `Vec(Some([Symbol(variant_name)]))`.
    fn enum_variant(name: &str) -> ScVal {
        ScVal::Vec(Some(ScVec(
            vec![ScVal::Symbol(ScSymbol(name.try_into().unwrap()))]
                .try_into()
                .unwrap(),
        )))
    }

    /// Build a `ScVal` mirroring how `#[contracttype]` encodes a struct: a
    /// `Map` keyed by field-name `Symbol`s. Only the fields the decoder reads
    /// are included; a real on-chain `Match` has many more.
    fn match_scval(id: u64, game_id: &str, platform_variant: &str) -> ScVal {
        let entries = vec![
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("game_id".try_into().unwrap())),
                val: ScVal::String(ScString(game_id.try_into().unwrap())),
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("id".try_into().unwrap())),
                val: ScVal::U64(id),
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("platform".try_into().unwrap())),
                val: enum_variant(platform_variant),
            },
        ];
        ScVal::Map(Some(ScMap(entries.try_into().unwrap())))
    }

    #[test]
    fn http_rpc_url_rejected_in_production_mode() {
        std::env::set_var("ORACLE_ENV", "production");
        let result = SorobanClient::new(
            "http://soroban-testnet.stellar.org".to_string(),
            "Test SDF Network ; September 2015".to_string(),
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );
        std::env::remove_var("ORACLE_ENV");
        assert!(
            result.is_err(),
            "expected HTTP rpc_url to be rejected in production mode"
        );
    }

    #[test]
    fn https_rpc_url_accepted_in_production_mode() {
        std::env::set_var("ORACLE_ENV", "production");
        let result = SorobanClient::new(
            "https://soroban-testnet.stellar.org".to_string(),
            "Test SDF Network ; September 2015".to_string(),
            "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );
        std::env::remove_var("ORACLE_ENV");
        assert!(
            result.is_ok() || matches!(result, Err(OracleServiceError::XdrError(_))),
            "https rpc_url should not be rejected for TLS reasons"
        );
    }

    #[test]
    fn decode_active_matches_happy_path() {
        let val = ScVal::Vec(Some(ScVec(
            vec![
                match_scval(1, "abcd1234", "Lichess"),
                match_scval(2, "efgh5678", "ChessDotCom"),
            ]
            .try_into()
            .unwrap(),
        )));

        let matches = decode_active_matches(&val).expect("decode should succeed");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].match_id, 1);
        assert_eq!(matches[0].game_id, "abcd1234");
        assert_eq!(matches[0].platform, Platform::Lichess);
        assert_eq!(matches[1].match_id, 2);
        assert_eq!(matches[1].game_id, "efgh5678");
        assert_eq!(matches[1].platform, Platform::ChessDotCom);
    }

    #[test]
    fn decode_active_matches_empty_vec() {
        let val = ScVal::Vec(Some(ScVec(VecM::default())));
        let matches = decode_active_matches(&val).expect("decode should succeed");
        assert!(matches.is_empty());
    }

    #[test]
    fn decode_active_matches_rejects_non_vec() {
        let err = decode_active_matches(&ScVal::Bool(true)).unwrap_err();
        assert!(matches!(err, OracleServiceError::XdrError(_)));
    }

    #[test]
    fn decode_match_summary_rejects_missing_field() {
        let entries = vec![
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("id".try_into().unwrap())),
                val: ScVal::U64(1),
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("platform".try_into().unwrap())),
                val: enum_variant("Lichess"),
            },
            // "game_id" deliberately omitted.
        ];
        let val = ScVal::Map(Some(ScMap(entries.try_into().unwrap())));
        let err = decode_match_summary(&val).unwrap_err();
        assert!(matches!(err, OracleServiceError::XdrError(_)));
    }

    #[test]
    fn decode_match_summary_rejects_unknown_platform_variant() {
        let val = match_scval(1, "abcd1234", "Xbox");
        let err = decode_match_summary(&val).unwrap_err();
        assert!(matches!(err, OracleServiceError::XdrError(_)));
    }
}
