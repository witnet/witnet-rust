use jsonrpc_pubsub as pubsub;

pub struct UnsubscribeRequest(pub pubsub::SubscriptionId);

pub type UnsubscribeResponse = ();
