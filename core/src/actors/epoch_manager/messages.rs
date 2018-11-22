use actix::dev::ToEnvelope;
use actix::{Actor, Addr, Handler, Message};

use super::{
    AllEpochSubscription, EpochManagerError, SendableNotification, SingleEpochSubscription,
};

use witnet_data_structures::chain::Epoch;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGES
////////////////////////////////////////////////////////////////////////////////////////
/// Returns the current epoch
pub struct GetEpoch;

/// Epoch result
pub type EpochResult<T> = Result<T, EpochManagerError>;

impl Message for GetEpoch {
    type Result = EpochResult<Epoch>;
}

/// Subscribe
pub struct Subscribe;

/// Subscribe to a single checkpoint
#[derive(Message)]
pub struct SubscribeEpoch {
    /// Checkpoint to be subscribed to
    pub checkpoint: Epoch,

    /// Notification to be sent when the checkpoint is reached
    pub notification: Box<dyn SendableNotification>,
}

/// Subscribe to all new checkpoints
#[derive(Message)]
pub struct SubscribeAll {
    /// Notification
    pub notification: Box<dyn SendableNotification>,
}

impl Subscribe {
    /// Subscribe to a specific checkpoint to get an EpochNotification
    // TODO: rename to to_checkpoint?
    // TODO: add helper Subscribe::to_next_epoch?
    // TODO: helper to subscribe to nth epoch in the future
    #[allow(clippy::wrong_self_convention)]
    pub fn to_epoch<T, U>(checkpoint: Epoch, addr: Addr<U>, payload: T) -> SubscribeEpoch
    where
        T: 'static,
        T: Send,
        U: Actor,
        U: Handler<EpochNotification<T>>,
        U::Context: ToEnvelope<U, EpochNotification<T>>,
    {
        SubscribeEpoch {
            checkpoint,
            notification: Box::new(SingleEpochSubscription {
                recipient: addr.recipient(),
                payload: Some(payload),
            }),
        }
    }
    /// Subscribe to all checkpoints to get an EpochNotification on every new epoch
    #[allow(clippy::wrong_self_convention)]
    pub fn to_all<T, U>(addr: Addr<U>, payload: T) -> SubscribeAll
    where
        T: 'static,
        T: Send + Clone,
        U: Actor,
        U: Handler<EpochNotification<T>>,
        U::Context: ToEnvelope<U, EpochNotification<T>>,
    {
        SubscribeAll {
            notification: Box::new(AllEpochSubscription {
                recipient: addr.recipient(),
                payload,
            }),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////
// MESSAGE FROM EPOCH MANAGER TO OTHER ACTORS
////////////////////////////////////////////////////////////////////////////////////////
/// Message that the EpochManager sends to subscriber actors to notify a new epoch
#[derive(Message)]
pub struct EpochNotification<T: Send> {
    /// Epoch that has just started
    pub checkpoint: Epoch,

    /// Payload for the epoch notification
    pub payload: T,
}
