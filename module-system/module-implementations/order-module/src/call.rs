use std::fmt::Debug;

use anyhow::Result;
#[cfg(feature = "native")]
use sov_modules_api::macros::CliWalletArg;
use sov_modules_api::prelude::*;
use sov_modules_api::{CallResponse, WorkingSet};
use thiserror::Error;

use super::OrderModule;

/// This enumeration represents the available call messages for interacting with the `sov-value-setter` module.
#[cfg_attr(feature = "native", derive(CliWalletArg), derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    derive(serde::Deserialize)
)]

#[derive(borsh::BorshDeserialize, borsh::BorshSerialize, Debug, Eq, PartialEq, Clone)]
pub enum CallMessage {
    /// Creates a new token with the specified name and initial balance.
    NewMarketOrder {
        /// asset to order with
        order_asset: String,
        /// asset to price with
        price_asset: String,
        /// 0 = bid, 1 = ask
        side: u32,
        /// quantity of order
        qty: u64,
        /// timestamp
        ts: u64,
    },
}

/// Example of a custom error.
#[derive(Debug, Error)]
enum NewOrderError {
    #[error("Only admin can create new order")]
    WrongSender,
}

impl<C: sov_modules_api::Context> OrderModule<C> {
    pub(crate) fn submit_order(
        &self,
        order_asset: String,
        price_asset: String,
        side: u32,
        qty: u64,
        ts: u64,
        context: &C,
        working_set: &mut WorkingSet<C>,
    ) -> Result<sov_modules_api::CallResponse> {
        // If admin is not then early return:
        let admin = self.admin.get_or_err(working_set)?;

        if &admin != context.sender() {
            // Here we use a custom error type.
            Err(NewOrderError::WrongSender)?;
        }

        let new_order_struct = CallMessage::NewMarketOrder {
            order_asset,
            price_asset,
            side,
            qty,
            ts,
        };

        let id: u64 = 12345678;

        // This is how we set a new value:
        self.orders.set(&id, &new_order_struct, working_set);
        working_set.add_event("set", &format!("order_set: {new_order_struct:?}"));

        Ok(CallResponse::default())
    }
}
