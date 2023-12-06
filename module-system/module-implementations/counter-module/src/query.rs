//! Defines rpc queries exposed by the module
use jsonrpsee::core::RpcResult;
use sov_modules_api::macros::rpc_gen;
use sov_modules_api::prelude::*;
use sov_modules_api::WorkingSet;

use super::CounterModule;

/// Response returned from the counter_queryCount endpoint.
#[derive(serde::Serialize, serde::Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Response {
    /// Value saved in the module's state.
    pub count: Option<u32>,
}

#[rpc_gen(client, server, namespace = "counter")]
impl<C: sov_modules_api::Context> CounterModule<C> {
    /// Queries the state of the module.
    #[rpc_method(name = "queryCount")]
    pub fn query_count(&self, working_set: &mut WorkingSet<C>) -> RpcResult<Response> {
        Ok(Response {
            count: self.count.get(working_set),
        })
    }
}
