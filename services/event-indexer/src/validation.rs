//! Input validation and sanitisation for the REST API.
//!
//! ## Why a middleware
//! Handlers used to receive whatever `serde` could deserialize, which meant a
//! malformed Stellar address or a negative amount reached the database (or the
//! contract) before anything noticed, and the caller got either a 500 with a
//! leaked driver message or axum's terse 422.  [`validate_request`] runs *before*
//! any handler, inspects the path and query string, and answers with a
//! `400 Bad Request` carrying a field-scoped message.
//!
//! ## Layering
//! The middleware is the outer guard; the individual `validate_*` functions are
//! also called directly by handlers that need the parsed value (a
//! `DateTime<Utc>`, an `i128`, …).  Validation is therefore *idempotent and
//! cheap* — never assume it only runs once.
//!
//! ## Stellar addresses
//! Length and alphabet checks alone accept typos.  [`validate_stellar_address`]
//! decodes the strkey (base32, no padding), checks the version byte (`G` =
//! ed25519 account, `C` = contract) and verifies the trailing CRC16-XMODEM
//! checksum, which is what actually catches a mistyped character.

use std::fmt;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};

use crate::api::ApiResponse;
use crate::models::MatchStatus;
use crate::transactions::{SortOrder, TransactionSortField, TransactionType};

// ── Limits ────────────────────────────────────────────────────────────────────

/// Length of a Stellar strkey (35 payload bytes → 56 base32 characters).
pub const STELLAR_ADDRESS_LEN: usize = 56;
/// Version byte for an ed25519 public key (`G…`).
pub const VERSION_BYTE_ACCOUNT: u8 = 6 << 3;
/// Version byte for a contract address (`C…`).
pub const VERSION_BYTE_CONTRACT: u8 = 2 << 3;

/// Smallest accepted page size.
pub const MIN_PAGE_LIMIT: i64 = 1;
/// Largest accepted page size — bounds the work a single request can cause.
pub const MAX_PAGE_LIMIT: i64 = 1_000;

/// Maximum accepted token amount, in stroops.
///
/// Soroban token amounts are `i128`, but no real balance approaches that; the
/// bound is `i64::MAX` stroops (≈ 9.2 × 10^11 whole units at 7 decimals), which
/// matches Stellar classic and keeps downstream arithmetic far from overflow.
pub const MAX_TOKEN_AMOUNT: i128 = i64::MAX as i128;

/// Maximum accepted `game_id` length.
pub const MAX_GAME_ID_LEN: usize = 64;
/// Maximum accepted token identifier length (a contract address, or a symbol).
pub const MAX_TOKEN_LEN: usize = 56;

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

// ── Error type ────────────────────────────────────────────────────────────────

/// A single validation failure, scoped to the offending field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        ValidationError {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

pub type ValidationResult<T> = Result<T, ValidationError>;

impl ValidationError {
    /// Render as the API's standard error envelope with a 400 status.
    pub fn into_response_tuple(self) -> (StatusCode, Json<ApiResponse<()>>) {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(self.to_string()),
            }),
        )
    }
}

impl IntoResponse for ValidationError {
    fn into_response(self) -> Response {
        self.into_response_tuple().into_response()
    }
}

// ── Stellar strkey ────────────────────────────────────────────────────────────

/// CRC16-XMODEM (polynomial `0x1021`, zero seed) — the checksum Stellar appends
/// to every strkey, stored little-endian.
fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for byte in data {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// Decode unpadded, uppercase base32 (RFC 4648) into bytes.
fn base32_decode(field: &str, value: &str) -> ValidationResult<Vec<u8>> {
    let mut out = Vec::with_capacity(value.len() * 5 / 8);
    let mut buffer: u16 = 0;
    let mut bits: u8 = 0;

    for ch in value.bytes() {
        let index = BASE32_ALPHABET
            .iter()
            .position(|c| *c == ch)
            .ok_or_else(|| {
                ValidationError::new(
                    field,
                    format!(
                        "must be base32-encoded (A–Z and 2–7 only); found {:?}",
                        ch as char
                    ),
                )
            })? as u16;

        buffer = (buffer << 5) | index;
        bits += 5;

        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }

    Ok(out)
}

/// Validate a Stellar strkey and return it unchanged.
///
/// Accepts both account (`G…`) and contract (`C…`) addresses.  Rejects wrong
/// length, wrong alphabet, unknown version byte and bad checksum.
pub fn validate_stellar_address(field: &str, value: &str) -> ValidationResult<String> {
    validate_strkey(field, value, &[VERSION_BYTE_ACCOUNT, VERSION_BYTE_CONTRACT])
}

/// Validate an account address (`G…`) — the form used for players.
pub fn validate_account_address(field: &str, value: &str) -> ValidationResult<String> {
    validate_strkey(field, value, &[VERSION_BYTE_ACCOUNT])
}

/// Validate a contract address (`C…`) — the form used for tokens and contracts.
pub fn validate_contract_address(field: &str, value: &str) -> ValidationResult<String> {
    validate_strkey(field, value, &[VERSION_BYTE_CONTRACT])
}

fn version_byte_label(version: u8) -> &'static str {
    match version {
        VERSION_BYTE_ACCOUNT => "G (account)",
        VERSION_BYTE_CONTRACT => "C (contract)",
        _ => "unknown",
    }
}

fn validate_strkey(field: &str, value: &str, allowed: &[u8]) -> ValidationResult<String> {
    if value.is_empty() {
        return Err(ValidationError::new(field, "must not be empty"));
    }

    if value.len() != STELLAR_ADDRESS_LEN {
        return Err(ValidationError::new(
            field,
            format!(
                "must be a {}-character Stellar address, got {} characters",
                STELLAR_ADDRESS_LEN,
                value.len()
            ),
        ));
    }

    if value.chars().any(|c| c.is_ascii_lowercase()) {
        return Err(ValidationError::new(
            field,
            "must be upper-case; Stellar addresses are case-sensitive",
        ));
    }

    let decoded = base32_decode(field, value)?;

    // 1 version byte + 32 key bytes + 2 checksum bytes.
    if decoded.len() != 35 {
        return Err(ValidationError::new(
            field,
            "is not a well-formed Stellar address (wrong payload length)",
        ));
    }

    let version = decoded[0];
    if !allowed.contains(&version) {
        let expected = allowed
            .iter()
            .map(|v| version_byte_label(*v))
            .collect::<Vec<_>>()
            .join(" or ");
        return Err(ValidationError::new(
            field,
            format!(
                "must be a {} address, got a {:?}-prefixed address",
                expected,
                value.chars().next().unwrap_or('?')
            ),
        ));
    }

    let (payload, checksum_bytes) = decoded.split_at(33);
    let expected = u16::from_le_bytes([checksum_bytes[0], checksum_bytes[1]]);
    if crc16_xmodem(payload) != expected {
        return Err(ValidationError::new(
            field,
            "has an invalid checksum — check for a typo",
        ));
    }

    Ok(value.to_string())
}

// ── Amounts ───────────────────────────────────────────────────────────────────

/// Validate a stroop amount supplied as a decimal string.
///
/// Rejects: empty input, signs, non-digits, zero, and values above
/// [`MAX_TOKEN_AMOUNT`].  Stakes and payouts are always strictly positive.
pub fn validate_token_amount(field: &str, raw: &str) -> ValidationResult<i128> {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Err(ValidationError::new(field, "must not be empty"));
    }

    if trimmed.starts_with('-') {
        return Err(ValidationError::new(
            field,
            "must be positive; negative amounts are not accepted",
        ));
    }

    if trimmed.starts_with('+') {
        return Err(ValidationError::new(
            field,
            "must not carry a sign; write the digits only",
        ));
    }

    if trimmed.contains('.') || trimmed.contains(',') {
        return Err(ValidationError::new(
            field,
            "must be an integer number of stroops (7 decimal places), not a decimal fraction",
        ));
    }

    if !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ValidationError::new(
            field,
            "must contain digits only",
        ));
    }

    let amount = trimmed.parse::<i128>().map_err(|_| {
        ValidationError::new(
            field,
            format!("is too large; the maximum is {} stroops", MAX_TOKEN_AMOUNT),
        )
    })?;

    if amount == 0 {
        return Err(ValidationError::new(field, "must be greater than zero"));
    }

    if amount > MAX_TOKEN_AMOUNT {
        return Err(ValidationError::new(
            field,
            format!("is too large; the maximum is {} stroops", MAX_TOKEN_AMOUNT),
        ));
    }

    Ok(amount)
}

// ── Identifiers ───────────────────────────────────────────────────────────────

/// Validate an off-chain game identifier (Lichess/Chess.com game id).
///
/// Allowed: ASCII alphanumerics, `-` and `_`, up to [`MAX_GAME_ID_LEN`] bytes.
/// Everything else — path separators, quotes, wildcards, control characters — is
/// rejected so the value can be used in a `LIKE`-free equality filter safely.
pub fn validate_game_id(field: &str, value: &str) -> ValidationResult<String> {
    if value.is_empty() {
        return Err(ValidationError::new(field, "must not be empty"));
    }

    if value.len() > MAX_GAME_ID_LEN {
        return Err(ValidationError::new(
            field,
            format!(
                "must be at most {} characters, got {}",
                MAX_GAME_ID_LEN,
                value.len()
            ),
        ));
    }

    if let Some(bad) = value
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_'))
    {
        return Err(ValidationError::new(
            field,
            format!(
                "may only contain letters, digits, '-' and '_'; found {:?}",
                bad
            ),
        ));
    }

    Ok(value.to_string())
}

/// Validate a match id.
///
/// Match ids are `u64` and start at **0** (`MatchCount` is initialised to zero
/// and the current count is used as the next id), so zero is valid.
pub fn validate_match_id(field: &str, raw: &str) -> ValidationResult<u64> {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Err(ValidationError::new(field, "must not be empty"));
    }

    if trimmed.starts_with('-') {
        return Err(ValidationError::new(
            field,
            "must not be negative; match ids are unsigned",
        ));
    }

    if !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ValidationError::new(
            field,
            "must be a whole number",
        ));
    }

    trimmed.parse::<u64>().map_err(|_| {
        ValidationError::new(field, "is out of range for a 64-bit match id")
    })
}

/// Validate a token filter: either a contract address or a short symbol.
pub fn validate_token(field: &str, value: &str) -> ValidationResult<String> {
    if value.is_empty() {
        return Err(ValidationError::new(field, "must not be empty"));
    }

    if value.len() > MAX_TOKEN_LEN {
        return Err(ValidationError::new(
            field,
            format!("must be at most {} characters", MAX_TOKEN_LEN),
        ));
    }

    // A 56-character value is meant to be a contract address; hold it to the
    // full strkey rules so a typo is reported instead of silently matching
    // nothing.
    if value.len() == STELLAR_ADDRESS_LEN {
        return validate_contract_address(field, value);
    }

    if let Some(bad) = value
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == ':' || *c == '-' || *c == '_'))
    {
        return Err(ValidationError::new(
            field,
            format!(
                "may only contain letters, digits, ':', '-' and '_'; found {:?}",
                bad
            ),
        ));
    }

    Ok(value.to_string())
}

// ── Enums ─────────────────────────────────────────────────────────────────────

pub fn validate_match_status(field: &str, raw: &str) -> ValidationResult<MatchStatus> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pending" => Ok(MatchStatus::Pending),
        "active" => Ok(MatchStatus::Active),
        "completed" => Ok(MatchStatus::Completed),
        "cancelled" => Ok(MatchStatus::Cancelled),
        "expired" => Ok(MatchStatus::Expired),
        other => Err(ValidationError::new(
            field,
            format!(
                "must be one of pending, active, completed, cancelled, expired; got {:?}",
                other
            ),
        )),
    }
}

pub fn validate_transaction_type(field: &str, raw: &str) -> ValidationResult<TransactionType> {
    TransactionType::parse(raw).ok_or_else(|| {
        ValidationError::new(
            field,
            format!("must be one of deposit, payout, fee; got {:?}", raw),
        )
    })
}

pub fn validate_sort_by(field: &str, raw: &str) -> ValidationResult<TransactionSortField> {
    TransactionSortField::parse(raw).ok_or_else(|| {
        ValidationError::new(
            field,
            format!(
                "must be one of timestamp, amount, match_id, type; got {:?}",
                raw
            ),
        )
    })
}

pub fn validate_sort_order(field: &str, raw: &str) -> ValidationResult<SortOrder> {
    SortOrder::parse(raw).ok_or_else(|| {
        ValidationError::new(field, format!("must be asc or desc; got {:?}", raw))
    })
}

// ── Pagination ────────────────────────────────────────────────────────────────

pub fn validate_limit(field: &str, raw: &str) -> ValidationResult<i64> {
    let value = parse_i64(field, raw)?;
    if value < MIN_PAGE_LIMIT || value > MAX_PAGE_LIMIT {
        return Err(ValidationError::new(
            field,
            format!(
                "must be between {} and {}, got {}",
                MIN_PAGE_LIMIT, MAX_PAGE_LIMIT, value
            ),
        ));
    }
    Ok(value)
}

pub fn validate_offset(field: &str, raw: &str) -> ValidationResult<i64> {
    let value = parse_i64(field, raw)?;
    if value < 0 {
        return Err(ValidationError::new(
            field,
            format!("must not be negative, got {}", value),
        ));
    }
    Ok(value)
}

fn parse_i64(field: &str, raw: &str) -> ValidationResult<i64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::new(field, "must not be empty"));
    }
    trimmed
        .parse::<i64>()
        .map_err(|_| ValidationError::new(field, format!("must be a whole number, got {:?}", raw)))
}

// ── Dates ─────────────────────────────────────────────────────────────────────

/// Parse an RFC 3339 / ISO 8601 timestamp, or a bare `YYYY-MM-DD` date, which is
/// interpreted as midnight UTC.
pub fn validate_date(field: &str, raw: &str) -> ValidationResult<DateTime<Utc>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::new(field, "must not be empty"));
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Ok(DateTime::from_naive_utc_and_offset(
            date.and_hms_opt(0, 0, 0)
                .expect("midnight is always a valid time"),
            Utc,
        ));
    }

    Err(ValidationError::new(
        field,
        format!(
            "must be an RFC 3339 timestamp (2026-07-27T12:00:00Z) or a date (2026-07-27); got {:?}",
            trimmed
        ),
    ))
}

/// Reject an inverted range.  Equal bounds are allowed (single instant).
pub fn validate_date_range(
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> ValidationResult<()> {
    if let (Some(from), Some(to)) = (from, to) {
        if from > to {
            return Err(ValidationError::new(
                "from_date",
                "must not be later than to_date",
            ));
        }
    }
    Ok(())
}

// ── Query-string scanning ─────────────────────────────────────────────────────

/// Decode one `application/x-www-form-urlencoded` component.
///
/// Kept local so the middleware does not need a URL-encoding dependency; invalid
/// escapes are passed through verbatim and then rejected by the field validator.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                match (hi, lo) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi * 16 + lo) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

/// Split a raw query string into decoded `(key, value)` pairs.
pub fn parse_query_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once('=') {
            Some((k, v)) => (percent_decode(k), percent_decode(v)),
            None => (percent_decode(part), String::new()),
        })
        .collect()
}

/// Validate one query parameter by name.
///
/// Unknown parameter names are accepted so that adding a parameter to a client
/// never needs a coordinated deploy; known names are validated strictly.
/// Empty values are treated as "not supplied" and skipped, matching how
/// `serde` handles an omitted optional field.
fn validate_query_param(key: &str, value: &str) -> ValidationResult<()> {
    if value.trim().is_empty() {
        return Ok(());
    }

    match key {
        "player_address" | "player" | "address" => {
            validate_stellar_address(key, value).map(|_| ())
        }
        "status" => validate_match_status(key, value).map(|_| ()),
        "limit" => validate_limit(key, value).map(|_| ()),
        "offset" => validate_offset(key, value).map(|_| ()),
        "game_id" => validate_game_id(key, value).map(|_| ()),
        "token" => validate_token(key, value).map(|_| ()),
        "amount" | "stake_amount" => validate_token_amount(key, value).map(|_| ()),
        "match_id" => validate_match_id(key, value).map(|_| ()),
        "type" | "tx_type" => validate_transaction_type(key, value).map(|_| ()),
        "sort_by" => validate_sort_by(key, value).map(|_| ()),
        "sort_order" | "order" => validate_sort_order(key, value).map(|_| ()),
        "from_date" | "to_date" | "start_date" | "end_date" => {
            validate_date(key, value).map(|_| ())
        }
        _ => Ok(()),
    }
}

/// Validate every recognised query parameter, then the cross-field date range.
pub fn validate_query_string(query: &str) -> ValidationResult<()> {
    let pairs = parse_query_pairs(query);

    for (key, value) in &pairs {
        validate_query_param(key, value)?;
    }

    // Cross-field: the range must not be inverted.  Both bounds have already
    // been proven parseable above.
    let lookup = |names: [&str; 2]| -> Option<DateTime<Utc>> {
        pairs
            .iter()
            .find(|(k, v)| names.contains(&k.as_str()) && !v.trim().is_empty())
            .and_then(|(k, v)| validate_date(k, v).ok())
    };

    validate_date_range(
        lookup(["from_date", "start_date"]),
        lookup(["to_date", "end_date"]),
    )
}

/// Validate the dynamic segments of a known route.
///
/// Unknown paths fall through untouched — routing, not validation, decides
/// whether they exist.
pub fn validate_path(path: &str) -> ValidationResult<()> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match segments.as_slice() {
        // /match/:match_id and /events/:match_id
        ["match", id] | ["events", id] => validate_match_id("match_id", id).map(|_| ()),
        // /transactions/player/:player_address
        ["transactions", "player", address] => {
            validate_account_address("player_address", address).map(|_| ())
        }
        _ => Ok(()),
    }
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Axum middleware that rejects malformed input with a `400` before any handler
/// runs.
///
/// Install with:
/// ```ignore
/// Router::new().layer(axum::middleware::from_fn(validate_request))
/// ```
pub async fn validate_request(req: Request, next: Next) -> Response {
    let uri = req.uri().clone();

    if let Err(e) = validate_path(uri.path()) {
        return e.into_response();
    }

    if let Some(query) = uri.query() {
        if let Err(e) = validate_query_string(query) {
            return e.into_response();
        }
    }

    next.run(req).await
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Real, checksum-valid mainnet-format account addresses.
    const VALID_ACCOUNT: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    const VALID_ACCOUNT_2: &str = "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H";
    /// Checksum-valid contract address.
    const VALID_CONTRACT: &str = "CAAACAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4DUPB6N4O";

    // ── Addresses ─────────────────────────────────────────────────────────

    #[test]
    fn valid_account_address_is_accepted() {
        assert_eq!(
            validate_stellar_address("player_address", VALID_ACCOUNT).unwrap(),
            VALID_ACCOUNT
        );
        assert!(validate_account_address("player_address", VALID_ACCOUNT_2).is_ok());
    }

    #[test]
    fn valid_contract_address_is_accepted() {
        assert!(validate_stellar_address("token", VALID_CONTRACT).is_ok());
        assert!(validate_contract_address("token", VALID_CONTRACT).is_ok());
    }

    #[test]
    fn checksum_verification_catches_a_single_character_typo() {
        // Flip the last character of a valid address; the alphabet and length are
        // still fine, only the CRC fails.
        let mut typo = VALID_ACCOUNT.to_string();
        typo.pop();
        typo.push('M');
        let err = validate_stellar_address("player_address", &typo).unwrap_err();
        assert!(
            err.message.contains("checksum"),
            "expected a checksum error, got {:?}",
            err.message
        );
    }

    #[test]
    fn too_short_address_is_rejected_with_the_length() {
        let err = validate_stellar_address("player_address", "GABC").unwrap_err();
        assert!(err.message.contains("56"));
        assert!(err.message.contains("got 4"));
    }

    #[test]
    fn too_long_address_is_rejected() {
        let long = format!("{}A", VALID_ACCOUNT);
        assert!(validate_stellar_address("player_address", &long).is_err());
    }

    #[test]
    fn empty_address_is_rejected() {
        let err = validate_stellar_address("player_address", "").unwrap_err();
        assert!(err.message.contains("empty"));
    }

    #[test]
    fn lowercase_address_is_rejected() {
        let err =
            validate_stellar_address("player_address", &VALID_ACCOUNT.to_lowercase()).unwrap_err();
        assert!(err.message.contains("upper-case"));
    }

    #[test]
    fn non_base32_characters_are_rejected() {
        // '0', '1' and '8' are not in the base32 alphabet.
        let mut bad = VALID_ACCOUNT.to_string();
        bad.replace_range(3..4, "0");
        let err = validate_stellar_address("player_address", &bad).unwrap_err();
        assert!(err.message.contains("base32"), "got {:?}", err.message);
    }

    #[test]
    fn ethereum_style_address_is_rejected() {
        let err = validate_stellar_address(
            "player_address",
            "0x71C7656EC7ab88b098defB751B7401B5f6d8976F",
        )
        .unwrap_err();
        // Wrong length is reported first.
        assert!(err.message.contains("56"));
    }

    #[test]
    fn contract_address_is_rejected_where_an_account_is_required() {
        let err = validate_account_address("player_address", VALID_CONTRACT).unwrap_err();
        assert!(
            err.message.contains("G (account)"),
            "got {:?}",
            err.message
        );
    }

    #[test]
    fn account_address_is_rejected_where_a_contract_is_required() {
        let err = validate_contract_address("token", VALID_ACCOUNT).unwrap_err();
        assert!(err.message.contains("C (contract)"), "got {:?}", err.message);
    }

    #[test]
    fn seed_prefixed_strkey_is_rejected() {
        // Version byte 'S' (secret seed) must never be accepted as an address.
        // Build one by re-encoding is unnecessary: any 56-char base32 string with
        // an S prefix fails either the version-byte or the checksum check.
        let mut seed = VALID_ACCOUNT.to_string();
        seed.replace_range(0..1, "S");
        assert!(validate_stellar_address("player_address", &seed).is_err());
    }

    // ── Amounts ───────────────────────────────────────────────────────────

    #[test]
    fn positive_amount_is_accepted() {
        assert_eq!(validate_token_amount("amount", "1").unwrap(), 1);
        assert_eq!(
            validate_token_amount("amount", "10000000").unwrap(),
            10_000_000
        );
        assert_eq!(
            validate_token_amount("amount", &MAX_TOKEN_AMOUNT.to_string()).unwrap(),
            MAX_TOKEN_AMOUNT
        );
    }

    #[test]
    fn negative_amount_is_rejected() {
        let err = validate_token_amount("amount", "-100").unwrap_err();
        assert!(err.message.contains("negative"), "got {:?}", err.message);
    }

    #[test]
    fn zero_amount_is_rejected() {
        let err = validate_token_amount("amount", "0").unwrap_err();
        assert!(err.message.contains("greater than zero"));
    }

    #[test]
    fn signed_positive_amount_is_rejected() {
        assert!(validate_token_amount("amount", "+100").is_err());
    }

    #[test]
    fn fractional_amount_is_rejected() {
        let err = validate_token_amount("amount", "1.5").unwrap_err();
        assert!(err.message.contains("stroops"), "got {:?}", err.message);
    }

    #[test]
    fn non_numeric_amount_is_rejected() {
        assert!(validate_token_amount("amount", "abc").is_err());
        assert!(validate_token_amount("amount", "1e10").is_err());
        assert!(validate_token_amount("amount", "").is_err());
        assert!(validate_token_amount("amount", "   ").is_err());
    }

    #[test]
    fn amount_above_the_maximum_is_rejected() {
        let too_big = (MAX_TOKEN_AMOUNT + 1).to_string();
        let err = validate_token_amount("amount", &too_big).unwrap_err();
        assert!(err.message.contains("too large"));
    }

    #[test]
    fn amount_beyond_i128_is_rejected_without_panicking() {
        let absurd = "9".repeat(60);
        assert!(validate_token_amount("amount", &absurd).is_err());
    }

    // ── Game ids ──────────────────────────────────────────────────────────

    #[test]
    fn typical_game_ids_are_accepted() {
        assert!(validate_game_id("game_id", "abcd1234").is_ok());
        assert!(validate_game_id("game_id", "game-001").is_ok());
        assert!(validate_game_id("game_id", "lichess_XyZ12345").is_ok());
        assert!(validate_game_id("game_id", "123456789").is_ok());
    }

    #[test]
    fn empty_game_id_is_rejected() {
        assert!(validate_game_id("game_id", "").is_err());
    }

    #[test]
    fn overlong_game_id_is_rejected() {
        let err = validate_game_id("game_id", &"a".repeat(MAX_GAME_ID_LEN + 1)).unwrap_err();
        assert!(err.message.contains("at most 64"));
    }

    #[test]
    fn game_id_with_injection_characters_is_rejected() {
        for bad in [
            "abc'; DROP TABLE events;--",
            "abc%",
            "../../etc/passwd",
            "abc def",
            "abc\0def",
            "<script>",
        ] {
            assert!(
                validate_game_id("game_id", bad).is_err(),
                "{:?} must be rejected",
                bad
            );
        }
    }

    // ── Match ids ─────────────────────────────────────────────────────────

    #[test]
    fn match_id_zero_is_valid() {
        // MatchCount starts at 0 and the current count becomes the next id.
        assert_eq!(validate_match_id("match_id", "0").unwrap(), 0);
    }

    #[test]
    fn negative_match_id_is_rejected() {
        let err = validate_match_id("match_id", "-1").unwrap_err();
        assert!(err.message.contains("negative"));
    }

    #[test]
    fn non_numeric_match_id_is_rejected() {
        assert!(validate_match_id("match_id", "abc").is_err());
        assert!(validate_match_id("match_id", "1;DROP").is_err());
        assert!(validate_match_id("match_id", "").is_err());
    }

    #[test]
    fn match_id_beyond_u64_is_rejected() {
        assert!(validate_match_id("match_id", &"9".repeat(25)).is_err());
    }

    // ── Tokens ────────────────────────────────────────────────────────────

    #[test]
    fn token_symbol_and_contract_address_are_accepted() {
        assert!(validate_token("token", "XLM").is_ok());
        assert!(validate_token("token", "USDC").is_ok());
        assert!(validate_token("token", VALID_CONTRACT).is_ok());
    }

    #[test]
    fn token_with_bad_characters_is_rejected() {
        assert!(validate_token("token", "XLM'; --").is_err());
        assert!(validate_token("token", "").is_err());
    }

    #[test]
    fn address_length_token_must_pass_checksum() {
        let mut typo = VALID_CONTRACT.to_string();
        typo.pop();
        typo.push('A');
        assert!(validate_token("token", &typo).is_err());
    }

    // ── Enums ─────────────────────────────────────────────────────────────

    #[test]
    fn valid_statuses_are_accepted_case_insensitively() {
        assert_eq!(
            validate_match_status("status", "pending").unwrap(),
            MatchStatus::Pending
        );
        assert_eq!(
            validate_match_status("status", "COMPLETED").unwrap(),
            MatchStatus::Completed
        );
    }

    #[test]
    fn unknown_status_lists_the_allowed_values() {
        let err = validate_match_status("status", "finished").unwrap_err();
        assert!(err.message.contains("pending"));
        assert!(err.message.contains("expired"));
    }

    #[test]
    fn transaction_type_validation() {
        assert_eq!(
            validate_transaction_type("type", "payout").unwrap(),
            TransactionType::Payout
        );
        let err = validate_transaction_type("type", "withdrawal").unwrap_err();
        assert!(err.message.contains("deposit, payout, fee"));
    }

    #[test]
    fn sort_validation() {
        assert!(validate_sort_by("sort_by", "amount").is_ok());
        assert!(validate_sort_by("sort_by", "; DROP TABLE").is_err());
        assert!(validate_sort_order("sort_order", "asc").is_ok());
        assert!(validate_sort_order("sort_order", "random()").is_err());
    }

    // ── Pagination ────────────────────────────────────────────────────────

    #[test]
    fn limit_bounds_are_enforced() {
        assert_eq!(validate_limit("limit", "1").unwrap(), 1);
        assert_eq!(validate_limit("limit", "1000").unwrap(), 1000);
        assert!(validate_limit("limit", "0").is_err());
        assert!(validate_limit("limit", "1001").is_err());
        assert!(validate_limit("limit", "-10").is_err());
        assert!(validate_limit("limit", "many").is_err());
    }

    #[test]
    fn limit_error_message_states_the_range() {
        let err = validate_limit("limit", "5000").unwrap_err();
        assert!(err.message.contains("between 1 and 1000"), "got {:?}", err.message);
    }

    #[test]
    fn offset_must_be_non_negative() {
        assert_eq!(validate_offset("offset", "0").unwrap(), 0);
        assert_eq!(validate_offset("offset", "500").unwrap(), 500);
        assert!(validate_offset("offset", "-1").is_err());
    }

    // ── Dates ─────────────────────────────────────────────────────────────

    #[test]
    fn rfc3339_and_plain_dates_are_accepted() {
        assert!(validate_date("from_date", "2026-07-27T12:00:00Z").is_ok());
        assert!(validate_date("from_date", "2026-07-27T12:00:00+02:00").is_ok());
        let midnight = validate_date("from_date", "2026-07-27").unwrap();
        assert_eq!(midnight.to_rfc3339(), "2026-07-27T00:00:00+00:00");
    }

    #[test]
    fn malformed_dates_are_rejected() {
        for bad in ["27-07-2026", "yesterday", "2026-13-01", "", "2026/07/27"] {
            assert!(
                validate_date("from_date", bad).is_err(),
                "{:?} must be rejected",
                bad
            );
        }
    }

    #[test]
    fn inverted_date_range_is_rejected() {
        let from = validate_date("from_date", "2026-07-27").unwrap();
        let to = validate_date("to_date", "2026-07-01").unwrap();
        let err = validate_date_range(Some(from), Some(to)).unwrap_err();
        assert_eq!(err.field, "from_date");
        assert!(err.message.contains("to_date"));
    }

    #[test]
    fn equal_bounds_and_open_ranges_are_allowed() {
        let d = validate_date("from_date", "2026-07-27").unwrap();
        assert!(validate_date_range(Some(d), Some(d)).is_ok());
        assert!(validate_date_range(Some(d), None).is_ok());
        assert!(validate_date_range(None, Some(d)).is_ok());
        assert!(validate_date_range(None, None).is_ok());
    }

    // ── Query strings ─────────────────────────────────────────────────────

    #[test]
    fn query_pairs_are_percent_decoded() {
        let pairs = parse_query_pairs("from_date=2026-07-27T12%3A00%3A00Z&limit=10");
        assert_eq!(pairs[0].1, "2026-07-27T12:00:00Z");
        assert_eq!(pairs[1].1, "10");
    }

    #[test]
    fn plus_decodes_to_space_in_query_values() {
        let pairs = parse_query_pairs("game_id=a+b");
        assert_eq!(pairs[0].1, "a b");
    }

    #[test]
    fn valid_query_string_passes() {
        let q = format!(
            "player_address={}&status=pending&limit=50&offset=0&type=deposit&token=XLM\
             &from_date=2026-01-01&to_date=2026-12-31&sort_by=amount&sort_order=asc",
            VALID_ACCOUNT
        );
        assert!(validate_query_string(&q).is_ok());
    }

    #[test]
    fn unknown_query_parameters_are_ignored() {
        assert!(validate_query_string("some_future_flag=1").is_ok());
    }

    #[test]
    fn empty_values_are_treated_as_absent() {
        assert!(validate_query_string("status=&limit=&player_address=").is_ok());
    }

    #[test]
    fn bad_query_parameter_reports_its_field() {
        let err = validate_query_string("limit=99999").unwrap_err();
        assert_eq!(err.field, "limit");

        let err = validate_query_string("player_address=NOPE").unwrap_err();
        assert_eq!(err.field, "player_address");

        let err = validate_query_string("status=finished").unwrap_err();
        assert_eq!(err.field, "status");
    }

    #[test]
    fn inverted_range_in_a_query_string_is_rejected() {
        let err = validate_query_string("from_date=2026-07-27&to_date=2026-07-01").unwrap_err();
        assert_eq!(err.field, "from_date");
    }

    // ── Paths ─────────────────────────────────────────────────────────────

    #[test]
    fn known_paths_validate_their_dynamic_segment() {
        assert!(validate_path("/match/1").is_ok());
        assert!(validate_path("/match/0").is_ok());
        assert!(validate_path("/events/42").is_ok());
        assert!(validate_path(&format!("/transactions/player/{}", VALID_ACCOUNT)).is_ok());
    }

    #[test]
    fn bad_path_segments_are_rejected() {
        assert!(validate_path("/match/abc").is_err());
        assert!(validate_path("/match/-1").is_err());
        assert!(validate_path("/events/1%20OR%201").is_err());
        assert!(validate_path("/transactions/player/NOT_AN_ADDRESS").is_err());
        assert!(validate_path(&format!("/transactions/player/{}", VALID_CONTRACT)).is_err());
    }

    #[test]
    fn unknown_paths_are_left_to_the_router() {
        assert!(validate_path("/health").is_ok());
        assert!(validate_path("/stats").is_ok());
        assert!(validate_path("/does/not/exist").is_ok());
    }

    // ── Error rendering ───────────────────────────────────────────────────

    #[test]
    fn error_display_includes_field_and_reason() {
        let err = ValidationError::new("limit", "must be between 1 and 1000, got 5000");
        assert_eq!(
            err.to_string(),
            "invalid limit: must be between 1 and 1000, got 5000"
        );
    }

    #[test]
    fn error_maps_to_400_with_the_standard_envelope() {
        let (status, Json(body)) =
            ValidationError::new("limit", "must be a whole number").into_response_tuple();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!body.success);
        assert!(body.data.is_none());
        assert_eq!(
            body.error.as_deref(),
            Some("invalid limit: must be a whole number")
        );
    }

    // ── Checksum primitive ────────────────────────────────────────────────

    #[test]
    fn crc16_matches_known_xmodem_vectors() {
        // Standard CRC16/XMODEM check value for "123456789" is 0x31C3.
        assert_eq!(crc16_xmodem(b"123456789"), 0x31C3);
        assert_eq!(crc16_xmodem(b""), 0x0000);
    }
}
