#![deny(missing_docs)]
#![doc = include_str!("../README.md")]
mod call;
mod genesis;

#[cfg(test)]
mod tests;

#[cfg(feature = "native")]
mod query;

pub use call::*;
pub use genesis::*;
#[cfg(feature = "native")]
pub use query::*;
use sov_modules_api::{CallResponse, Error, ModuleInfo, WorkingSet};

/// A new module:
/// - Must derive `ModuleInfo`
/// - Must contain `[address]` field
/// - Can contain any number of ` #[state]` or `[module]` fields
#[cfg_attr(feature = "native", derive(sov_modules_api::ModuleCallJsonSchema))]
#[derive(ModuleInfo)]
pub struct OrderModule<C: sov_modules_api::Context> {
    /// Address of the module.
    #[address]
    pub address: C::Address,

    /// order kept in the state.
    #[state]
    pub orders: sov_modules_api::StateMap<u64, crate::CallMessage>,

    /// Holds the address of the admin user who is allowed to update the value.
    #[state]
    pub admin: sov_modules_api::StateValue<C::Address>,
}

impl<C: sov_modules_api::Context> sov_modules_api::Module for OrderModule<C> {
    type Context = C;

    type Config = OrderModuleConfig<C>;

    type CallMessage = call::CallMessage;

    type Event = ();

    fn genesis(&self, config: &Self::Config, working_set: &mut WorkingSet<C>) -> Result<(), Error> {
        // The initialization logic
        Ok(self.init_module(config, working_set)?)
    }

    fn call(
        &self,
        msg: Self::CallMessage,
        context: &Self::Context,
        working_set: &mut WorkingSet<C>,
    ) -> Result<sov_modules_api::CallResponse, Error> {
        match msg {
            call::CallMessage::NewMarketOrder {
                order_asset,
                price_asset,
                side,
                qty,
                ts,
            } => {
                self.submit_order(
                    order_asset,
                    price_asset,
                    side,
                    qty,
                    ts,
                    context,
                    working_set,
                )?;
                Ok(CallResponse::default())
            }
        }
    }
}
