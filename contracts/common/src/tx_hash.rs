//! Validation for Stellar transaction hashes.
//!
//! A Stellar transaction hash is the hex-encoded SHA-256 digest of the
//! transaction envelope: exactly 64 hexadecimal characters. Hashes are
//! canonically stored and compared in lowercase, so [`validate_tx_hash`]
//! accepts mixed-case input and returns the normalized form.

/// Length of a Stellar transaction hash in hexadecimal characters.
pub const TX_HASH_LEN: usize = 64;

/// Errors returned by [`validate_tx_hash`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TxHashError {
    /// The hash is an empty string.
    Empty,
    /// The hash is not exactly [`TX_HASH_LEN`] bytes long.
    InvalidLength,
    /// The hash contains a byte outside `0-9`, `a-f`, and `A-F`.
    InvalidCharacter,
}

impl std::fmt::Display for TxHashError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxHashError::Empty => write!(f, "transaction hash is empty"),
            TxHashError::InvalidLength => {
                write!(f, "transaction hash must be {TX_HASH_LEN} hex characters")
            }
            TxHashError::InvalidCharacter => {
                write!(f, "transaction hash contains a non-hex character")
            }
        }
    }
}

impl std::error::Error for TxHashError {}

/// Validates a Stellar transaction hash and returns its canonical lowercase
/// form, suitable for storage in payment and monitor records and for
/// building explorer links.
///
/// # Errors
///
/// - [`TxHashError::Empty`] if `hash` is empty.
/// - [`TxHashError::InvalidLength`] if `hash` is not exactly
///   [`TX_HASH_LEN`] bytes long.
/// - [`TxHashError::InvalidCharacter`] if `hash` contains a byte outside
///   `0-9`, `a-f`, and `A-F`.
pub fn validate_tx_hash(hash: &str) -> Result<String, TxHashError> {
    if hash.is_empty() {
        return Err(TxHashError::Empty);
    }
    if hash.len() != TX_HASH_LEN {
        return Err(TxHashError::InvalidLength);
    }
    if !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(TxHashError::InvalidCharacter);
    }
    Ok(hash.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_LOWERCASE: &str =
        "7b90f3a2c1d4e5f60718293a4b5c6d7e8f901234567890123456789012345678";
    const VALID_UPPERCASE: &str =
        "7B90F3A2C1D4E5F60718293A4B5C6D7E8F901234567890123456789012345678";

    #[test]
    fn test_validate_tx_hash_valid_lowercase_returns_ok() {
        let result = validate_tx_hash(VALID_LOWERCASE);
        assert_eq!(result, Ok(VALID_LOWERCASE.to_string()));
    }

    #[test]
    fn test_validate_tx_hash_uppercase_is_normalized_to_lowercase() {
        let result = validate_tx_hash(VALID_UPPERCASE);
        assert_eq!(result, Ok(VALID_LOWERCASE.to_string()));
    }

    #[test]
    fn test_validate_tx_hash_mixed_case_is_normalized_to_lowercase() {
        let mixed = "7b90F3a2C1d4E5f60718293A4b5C6d7E8f901234567890123456789012345678";
        let result = validate_tx_hash(mixed);
        assert_eq!(result, Ok(VALID_LOWERCASE.to_string()));
    }

    #[test]
    fn test_validate_tx_hash_all_zero_hash_is_valid() {
        let zeros = "0".repeat(TX_HASH_LEN);
        let result = validate_tx_hash(&zeros);
        assert_eq!(result, Ok(zeros));
    }

    #[test]
    fn test_validate_tx_hash_empty_returns_empty_error() {
        let result = validate_tx_hash("");
        assert_eq!(result, Err(TxHashError::Empty));
    }

    #[test]
    fn test_validate_tx_hash_too_short_returns_invalid_length() {
        let result = validate_tx_hash(&VALID_LOWERCASE[..TX_HASH_LEN - 1]);
        assert_eq!(result, Err(TxHashError::InvalidLength));
    }

    #[test]
    fn test_validate_tx_hash_too_long_returns_invalid_length() {
        let mut too_long = String::from(VALID_LOWERCASE);
        too_long.push('a');
        let result = validate_tx_hash(&too_long);
        assert_eq!(result, Err(TxHashError::InvalidLength));
    }

    #[test]
    fn test_validate_tx_hash_non_hex_character_returns_invalid_character() {
        let result = validate_tx_hash(&VALID_LOWERCASE.replace('7', "g"));
        assert_eq!(result, Err(TxHashError::InvalidCharacter));
    }

    #[test]
    fn test_validate_tx_hash_embedded_space_returns_invalid_character() {
        let result = validate_tx_hash(&VALID_LOWERCASE.replace('7', " "));
        assert_eq!(result, Err(TxHashError::InvalidCharacter));
    }
}
