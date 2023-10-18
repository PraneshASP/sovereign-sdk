use std::collections::HashMap;
use std::hash::Hash;

use jmt::storage::{LeafNode, Node, NodeKey, TreeReader, TreeWriter};
use jmt::{KeyHash, OwnedValue, Version};
// use std::sync::{Arc, RwLock};
use sov_rollup_interface::da::{BlockHeaderTrait, DaSpec};
use sov_state::storage::{Snapshot, SnapshotId, SnapshotQuery, StorageKey, StorageValue};

#[derive(Debug)]
pub struct ForkManager<S: Snapshot, Da: DaSpec> {
    // Storage actually needed only to commit data to the database.
    // So technically we can extract it and "finalize" method here will just
    #[allow(dead_code)]
    db: sov_db::state_db::StateDB,
    #[allow(dead_code)]
    native_db: sov_db::native_db::NativeDB,

    snapshots: HashMap<Da::SlotHash, S>,

    // L1 forks representation
    // Chain: prev_block -> child_blocks
    chain_forks: HashMap<Da::SlotHash, Vec<Da::SlotHash>>,
    // Reverse: child_block -> parent
    blocks_to_parent: HashMap<Da::SlotHash, Da::SlotHash>,

    // Helper mappings
    latest_snapshot_id: SnapshotId,
    snapshot_id_to_block_hash: HashMap<SnapshotId, Da::SlotHash>,
}

pub struct SnapshotParentIterator<'a, S, Da>
where
    S: Snapshot,
    Da: DaSpec,
{
    manager: &'a ForkManager<S, Da>,
    current_block_hash: Option<Da::SlotHash>,
}

impl<'a, S, Da> Iterator for SnapshotParentIterator<'a, S, Da>
where
    S: Snapshot,
    Da: DaSpec,
    Da::SlotHash: Hash,
{
    type Item = &'a S;

    fn next(&mut self) -> Option<Self::Item> {
        let current_hash = self.current_block_hash.as_ref()?;

        let snapshot = self.manager.snapshots.get(current_hash)?;

        self.current_block_hash = self.manager.blocks_to_parent.get(current_hash).cloned();

        Some(snapshot)
    }
}

impl<S, Da> SnapshotQuery for ForkManager<S, Da>
where
    S: Snapshot,
    Da: DaSpec,
    Da::SlotHash: Hash,
{
    fn query_storage_value(
        &self,
        snapshot_id: &SnapshotId,
        key: &StorageKey,
    ) -> Option<StorageValue> {
        for snapshot in self.parent_iterator(snapshot_id) {
            let value = snapshot.get_storage_value(key);
            if value.is_some() {
                return value;
            }
        }
        None
    }

    fn query_accessory_value(
        &self,
        snapshot_id: &SnapshotId,
        key: &StorageKey,
    ) -> Option<StorageValue> {
        for snapshot in self.parent_iterator(snapshot_id) {
            let value = snapshot.get_accessory_value(key);
            if value.is_some() {
                return value;
            }
        }
        None
    }

    fn query_node_option(
        &self,
        snapshot_id: &SnapshotId,
        node_key: &NodeKey,
    ) -> anyhow::Result<Option<Node>> {
        for snapshot in self.parent_iterator(snapshot_id) {
            let node = snapshot.get_node_option(node_key)?;
            if node.is_some() {
                return Ok(node);
            }
        }
        Ok(None)
    }

    fn query_value_option(
        &self,
        snapshot_id: &SnapshotId,
        max_version: Version,
        key_hash: KeyHash,
    ) -> anyhow::Result<Option<OwnedValue>> {
        for snapshot in self.parent_iterator(snapshot_id) {
            let value = snapshot.get_value_option(max_version, key_hash)?;
            if value.is_some() {
                return Ok(value);
            }
        }
        Ok(None)
    }
}

impl<S, Da> ForkManager<S, Da>
where
    S: Snapshot,
    Da: DaSpec,
    Da::SlotHash: Hash,
{
    pub fn new(db: sov_db::state_db::StateDB, native_db: sov_db::native_db::NativeDB) -> Self {
        Self {
            db,
            native_db,
            chain_forks: Default::default(),
            blocks_to_parent: Default::default(),
            snapshots: Default::default(),
            snapshot_id_to_block_hash: Default::default(),
            latest_snapshot_id: Default::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.chain_forks.is_empty()
            && self.blocks_to_parent.is_empty()
            && self.snapshots.is_empty()
            && self.snapshot_id_to_block_hash.is_empty()
    }

    pub fn parent_iterator(&self, snapshot_id: &SnapshotId) -> SnapshotParentIterator<S, Da> {
        let current_block_hash = self.snapshot_id_to_block_hash.get(snapshot_id).cloned();
        SnapshotParentIterator {
            manager: self,
            current_block_hash,
        }
    }

    pub fn get_new_ref(&mut self, block_header: &Da::BlockHeader) -> SnapshotId {
        self.latest_snapshot_id += 1;

        let current_block_hash = block_header.hash();
        let prev_block_hash = block_header.prev_hash();
        //
        self.snapshot_id_to_block_hash
            .insert(self.latest_snapshot_id, current_block_hash.clone());
        //
        let c = self
            .blocks_to_parent
            .insert(current_block_hash.clone(), prev_block_hash.clone());
        // // TODO: Maybe assert that parent is the same? Then
        assert!(
            c.is_none(),
            "current block hash has already snapshot requested"
        );
        self.chain_forks
            .entry(prev_block_hash)
            .or_default()
            .push(current_block_hash);

        self.latest_snapshot_id
    }

    pub fn add_snapshot(&mut self, snapshot: S) {
        let snapshot_block_hash = self
            .snapshot_id_to_block_hash
            .get(&snapshot.get_id())
            .unwrap();
        self.snapshots.insert(snapshot_block_hash.clone(), snapshot);
    }
}

impl<S, Da> ForkManager<S, Da>
where
    S: Snapshot + Into<(jmt::storage::NodeBatch, sov_state::OrderedReadsAndWrites)>,
    Da: DaSpec,
    Da::SlotHash: Hash,
{
    fn remove_snapshot(&mut self, block_hash: &Da::SlotHash) -> S {
        let snapshot = self
            .snapshots
            .remove(block_hash)
            .expect("Tried to remove non-existing snapshot: self.snapshots");
        let _removed_block_hash = self
            .snapshot_id_to_block_hash
            .remove(&snapshot.get_id())
            .unwrap();
        debug_assert_eq!(&_removed_block_hash, block_hash, "database is inconsistent");
        snapshot
    }

    fn commit_snapshot(&self, snapshot: S) {
        let (node_batch, accessory_writes) = snapshot.into();
        {
            // TODO: Populate pre-image for keys here.
            self.db
                .write_node_batch(&node_batch)
                .expect("db write must succeed");

            self.native_db
                .set_values(
                    accessory_writes
                        .ordered_writes
                        .iter()
                        .map(|(k, v_opt)| {
                            (k.key.to_vec(), v_opt.as_ref().map(|v| v.value.to_vec()))
                        })
                        .collect(),
                )
                .expect("native db write must succeed");

            self.db.inc_next_version();
        }
    }

    pub fn finalize_snapshot(&mut self, block_hash: &Da::SlotHash) {
        let snapshot = self.remove_snapshot(block_hash);
        self.commit_snapshot(snapshot);

        if let Some(parent_block_hash) = self.blocks_to_parent.remove(block_hash) {
            let mut to_discard: Vec<_> = self
                .chain_forks
                .remove(&parent_block_hash)
                .expect("Inconsistent chain_forks")
                .into_iter()
                .filter(|bh| bh != block_hash)
                .collect();
            while let Some(next_to_discard) = to_discard.pop() {
                let next_children_to_discard = self
                    .chain_forks
                    .remove(&next_to_discard)
                    .unwrap_or_default();
                to_discard.extend(next_children_to_discard);

                self.blocks_to_parent.remove(&next_to_discard).unwrap();
                self.remove_snapshot(&next_to_discard);
            }
        }
    }
}

/// OPTION WITH TRAIT
pub trait ForkManagerTrait<Da: DaSpec> {
    type Snapshot;
    type Query;
    fn get_new_ref(&mut self, block_header: &Da::BlockHeader) -> Self::Query;
    fn add_snapshot(&mut self, snapshot: Self::Snapshot);
    fn finalize_snapshot(&mut self, block_hash: &Da::SlotHash);
}

#[cfg(test)]
mod tests {
    use tempfile;

    use super::*;
    type Da = sov_rollup_interface::mocks::MockDaSpec;

    struct MockSnapshot {
        id: SnapshotId,
        cache: HashMap<Vec<u8>, Vec<u8>>,
        accessory_cache: HashMap<Vec<u8>, Vec<u8>>,
    }

    impl TreeReader for MockSnapshot {
        fn get_node_option(&self, node_key: &NodeKey) -> anyhow::Result<Option<Node>> {
            todo!()
        }

        fn get_value_option(
            &self,
            max_version: Version,
            key_hash: KeyHash,
        ) -> anyhow::Result<Option<OwnedValue>> {
            todo!()
        }

        fn get_rightmost_leaf(&self) -> anyhow::Result<Option<(NodeKey, LeafNode)>> {
            todo!()
        }
    }

    impl Snapshot for MockSnapshot {
        fn get_storage_value(&self, key: &StorageKey) -> Option<StorageValue> {
            let key = (*key.key()).clone();
            self.cache.get(&key).cloned().map(|v| StorageValue::from(v))
        }

        fn get_accessory_value(&self, key: &StorageKey) -> Option<StorageValue> {
            let key = (*key.key()).clone();
            self.accessory_cache
                .get(&key)
                .cloned()
                .map(|v| StorageValue::from(v))
        }

        fn get_id(&self) -> SnapshotId {
            self.id
        }
    }

    #[test]
    fn initiate_new() {
        let tmpdir = tempfile::tempdir().unwrap();

        let db = sov_db::state_db::StateDB::with_path(tmpdir.path()).unwrap();
        let native_db = sov_db::native_db::NativeDB::with_path(tmpdir.path()).unwrap();
        let fork_manager = ForkManager::<MockSnapshot, Da>::new(db, native_db);
        assert!(fork_manager.is_empty());
    }

    #[test]
    #[ignore = "TBD"]
    fn linear_progression_with_2_blocks_delay() {}

    #[test]
    #[ignore = "TBD"]
    fn fork_added() {}

    #[test]
    #[ignore = "TBD"]
    fn adding_alien_snapshot() {}

    #[test]
    #[ignore = "TBD"]
    fn finalizing_alien_block() {}

    #[test]
    #[ignore = "TBD"]
    fn finalizing_same_block_hash_twice() {}

    #[test]
    #[ignore = "TBD"]
    fn requesting_ref_from_same_block_twice() {}
}
