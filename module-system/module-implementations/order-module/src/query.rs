//! Defines rpc queries exposed by the module
use jsonrpsee::core::RpcResult;
use sov_modules_api::macros::rpc_gen;
use sov_modules_api::prelude::*;
use sov_modules_api::WorkingSet;

pub use crate::call::*;

use super::OrderModule;

/// Response returned from the order_queryCount endpoint.
#[derive(serde::Serialize, serde::Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Response  {
    /// Value saved in the module's state.
    pub order: CallMessage,
}

#[rpc_gen(client, server, namespace = "order")]
impl<C: sov_modules_api::Context> OrderModule<C> {
    /// Queries the state of the module.
    #[rpc_method(name = "queryOrder")]
    pub fn query_order(&self, id: u64, working_set: &mut WorkingSet<C>) -> RpcResult<Response> {
        let order = match self.orders.get(&id, working_set) {
            None => {
                // anyhow::bail!("Order with id {} does not exist", id);
                panic!("Order with id {} does not exist", id);
            }
            Some(order) => order,
        };
        println!("order found: {:?}", order);
        Ok(Response {
            order,
        })
    }
}
