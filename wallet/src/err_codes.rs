//! Custom error codes for JsonRPC implementation

/// Indicates there was an error in the communication with the Node.
pub static NODE_ERROR: i64 = -32000;

/// Indicates there was an error serializing the data for the response.
pub static SERIALIZATION_ERROR: i64 = -32001;

/// Indicates there was an error due to the malfunction of the application.
pub static INTERNAL_ERROR: i64 = -32002;
