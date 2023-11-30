//! State manager for [`ProverStorage`]

use crate::{config, MerkleProofSpec, ProverStorage};

/// State manager for Prover and Zk Storage
pub struct ProverStorageManager<S: MerkleProofSpec, Q> {
    state_db: sov_db::state_db::StateDB,
    native_db: sov_db::native_db::NativeDB,
    phantom_s: std::marker::PhantomData<S>,
    phantom_q: std::marker::PhantomData<Q>,
}

impl<S: MerkleProofSpec, Q> ProverStorageManager<S, Q> {
    /// Create new [`ProverStorageManager`] from state config
    pub fn new(config: config::Config) -> anyhow::Result<Self> {
        let path = config.path;
        let state_db = sov_db::state_db::StateDB::with_path(&path)?;
        let native_db = sov_db::native_db::NativeDB::with_path(&path)?;
        Ok(Self {
            state_db,
            native_db,
            phantom_s: Default::default(),
            phantom_q: Default::default(),
        })
    }
}

impl<S: MerkleProofSpec, Q> sov_rollup_interface::storage::StorageManager
    for ProverStorageManager<S, Q>
{
    type NativeStorage = ProverStorage<S, Q>;
    type NativeChangeSet = ();
    fn get_native_storage(&self) -> Self::NativeStorage {
        ProverStorage::with_db_handles(self.state_db.clone(), self.native_db.clone())
    }
}
