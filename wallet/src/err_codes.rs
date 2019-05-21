//! Custom error codes for JsonRPC implementation

/// Indicates there was an error in the communication with the Node.
pub static NODE_ERROR: i64 = -32010;

/// Indicates there was an error due to the malfunction of the application.
pub static INTERNAL_ERROR: i64 = -32011;

/// Indicates there was an error due to some timeout inside the application.
pub static TIMEOUT_ERROR: i64 = -32012;

/// Indicates there was an error during the execution of a RAD-request
pub static RAD_ERROR: i64 = -32030;
