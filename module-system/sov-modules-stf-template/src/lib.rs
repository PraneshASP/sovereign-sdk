pub mod app_template;
mod batch;
pub mod sync_strategies;
mod tx_verifier;

pub use app_template::AppTemplate;
pub use batch::Batch;
use tracing::log::info;
pub use tx_verifier::RawTx;

use sov_modules_api::{
    hooks::{ApplyBlobHooks, SyncHooks, TxHooks},
    Context, DispatchCall, Genesis, Spec,
};
use sov_rollup_interface::stf::StateTransitionFunction;
use sov_rollup_interface::stf::{BatchReceipt, SyncReceipt};
use sov_rollup_interface::zk::traits::Zkvm;
use sov_state::StateCheckpoint;
use sov_state::Storage;
use std::io::Read;

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TxEffect {
    Reverted,
    Successful,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SenderOutcome {
    /// Sequencer receives reward amount in defined token and can withdraw its deposit
    Rewarded(u64),
    /// Sequencer loses its deposit and receives no reward
    Slashed(SlashingReason),
    /// Batch was ignored, sequencer deposit left untouched.
    Ignored,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SlashingReason {
    InvalidBatchEncoding,
    StatelessVerificationFailed,
    InvalidTransactionEncoding,
}

impl<C: Context, RT, Vm: Zkvm> StateTransitionFunction<Vm> for AppTemplate<C, RT, Vm>
where
    RT: DispatchCall<Context = C>
        + Genesis<Context = C>
        + TxHooks<Context = C>
        + ApplyBlobHooks<Context = C, BlobResult = SenderOutcome>
        + SyncHooks<Context = C>,
{
    type StateRoot = jmt::RootHash;

    type InitialState = <RT as Genesis>::Config;

    type TxReceiptContents = TxEffect;

    type BatchReceiptContents = SenderOutcome;

    type Witness = <<C as Spec>::Storage as Storage>::Witness;

    type MisbehaviorProof = ();

    fn init_chain(&mut self, params: Self::InitialState) {
        let mut working_set = StateCheckpoint::new(self.current_storage.clone()).to_revertable();

        self.runtime
            .genesis(&params, &mut working_set)
            .expect("module initialization must succeed");

        let (log, witness) = working_set.checkpoint().freeze();
        self.current_storage
            .validate_and_commit(log, &witness)
            .expect("Storage update must succeed");
    }

    fn begin_slot(&mut self, witness: Self::Witness) {
        self.checkpoint = Some(StateCheckpoint::with_witness(
            self.current_storage.clone(),
            witness,
        ));
    }

    fn apply_tx_blob(
        &mut self,
        blob: &mut impl sov_rollup_interface::da::BlobTransactionTrait,
        _misbehavior_hint: Option<Self::MisbehaviorProof>,
    ) -> BatchReceipt<Self::BatchReceiptContents, Self::TxReceiptContents> {
        match self.apply_tx_blob(blob) {
            Ok(batch) => batch,
            Err(e) => e.into(),
        }
    }

    fn end_slot(&mut self) -> (Self::StateRoot, Self::Witness) {
        let (cache_log, witness) = self.checkpoint.take().unwrap().freeze();
        let root_hash = self
            .current_storage
            .validate_and_commit(cache_log, &witness)
            .expect("jellyfish merkle tree update must succeed");
        (jmt::RootHash(root_hash), witness)
    }

    type SyncReceiptContents = SenderOutcome;

    fn apply_sync_data_blob(
        &mut self,
        blob: &mut impl sov_rollup_interface::da::BlobTransactionTrait,
    ) -> sov_rollup_interface::stf::SyncReceipt<Self::SyncReceiptContents> {
        let mut batch_workspace = self
            .checkpoint
            .take()
            .expect("Working_set was initialized in begin_slot")
            .to_revertable();

        let address = match self.runtime.pre_blob_hook(blob, &mut batch_workspace) {
            Ok(address) => address,
            Err(e) => {
                info!("Sync pre-blob hook rejected: {:?}", e);
                return SyncReceipt {
                    blob_hash: blob.hash(),
                    inner: SenderOutcome::Ignored,
                };
            }
        };

        let data = blob.data_mut();
        let mut contiguous_data = Vec::with_capacity(data.total_len());
        data.read_to_end(&mut contiguous_data)
            .expect("Reading from blob should succeed");

        let decoded = RT::decode_call(&contiguous_data);
        match decoded {
            Ok(call) => {
                // TODO: do something with this result
                let _ =
                    self.runtime
                        .dispatch_call(call, &mut batch_workspace, &Context::new(address));
            }
            Err(e) => {
                info!("Sync data blob decoding failed: {:?}", e);
                return SyncReceipt {
                    blob_hash: blob.hash(),
                    inner: SenderOutcome::Slashed(SlashingReason::InvalidBatchEncoding),
                };
            }
        };
        // TODO: Make the reward sensible
        SyncReceipt {
            blob_hash: blob.hash(),
            inner: SenderOutcome::Rewarded(0),
        }
    }
}
