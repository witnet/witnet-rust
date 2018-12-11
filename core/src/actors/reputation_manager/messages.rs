use actix::Message;

/// Message for checking that a proof of eligibility is valid for the
/// known reputation of the issuer of the proof
#[derive(Debug)]
pub struct ValidatePoE;

impl Message for ValidatePoE {
    type Result = bool;
}
