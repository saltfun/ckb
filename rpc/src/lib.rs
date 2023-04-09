//! See [module](module/index.html) for the RPC methods documentation.

pub(crate) mod error;
pub(crate) mod server;
pub(crate) mod service_builder;
pub(crate) mod util;

pub mod module;

#[cfg(test)]
mod tests;

pub use crate::error::RPCError;
pub use crate::server::RpcServer;
pub use crate::service_builder::ServiceBuilder;

#[doc(hidden)]
pub type IoHandler = jsonrpc_pubsub::PubSubHandler<Option<crate::module::SubscriptionSession>>;
