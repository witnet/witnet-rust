use std::fmt;

#[derive(Debug)]
pub enum ScriptError {
    Decode(serde_json::Error),
    Encode(serde_json::Error),
    EmptyStackPop,
    VerifyOpFailed,
    IfNotBoolean,
    UnbalancedElseOp,
    UnbalancedEndIfOp,
    UnexpectedArgument,
    InvalidSignature,
    InvalidPublicKey,
    InvalidPublicKeyHash,
    WrongSignaturePublicKey,
    BadNumberPublicKeysInMultiSig,
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::Decode(e) => write!(f, "Decode script failed: {}", e),
            ScriptError::Encode(e) => write!(f, "Encode script failed: {}", e),
            ScriptError::EmptyStackPop => write!(f, "Tried to pop value from empty stack"),
            ScriptError::VerifyOpFailed => write!(f, "Verify operator input was not true"),
            ScriptError::IfNotBoolean => write!(f, "Input of If operator was not a boolean"),
            ScriptError::UnbalancedElseOp => write!(f, "Else operator is not inside an if block"),
            ScriptError::UnbalancedEndIfOp => write!(
                f,
                "EndIf operator does not have a corresponding If operator"
            ),
            ScriptError::UnexpectedArgument => write!(f, "Stack item had an invalid type"),
            ScriptError::InvalidSignature => write!(f, "Invalid signature serialization"),
            ScriptError::InvalidPublicKey => write!(f, "Invalid PublicKey serialization"),
            ScriptError::InvalidPublicKeyHash => write!(f, "Invalid PublicKeyHash serialization"),
            ScriptError::WrongSignaturePublicKey => write!(
                f,
                "The public key used by this signature was not the expected public key"
            ),
            ScriptError::BadNumberPublicKeysInMultiSig => {
                write!(f, "Invalid number of public keys in MultiSig")
            }
        }
    }
}

impl std::error::Error for ScriptError {}
