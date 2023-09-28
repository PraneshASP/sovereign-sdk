use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::routing;
use axum_test::TestServer;
use demo_stf::app::{App, DefaultPrivateKey};
use demo_stf::genesis_config::create_demo_genesis_config;
use sov_db::ledger_db::{LedgerDB, SlotCommit};
use sov_modules_api::PrivateKey;
use sov_risc0_adapter::host::Risc0Verifier;
use sov_rng_da_service::{RngDaService, RngDaSpec};
use sov_rollup_interface::digest::typenum::Le;
use sov_rollup_interface::mocks::{MockAddress, MockBlock, MockBlockHeader};
use sov_rollup_interface::services::da::DaService;
use sov_rollup_interface::stf::StateTransitionFunction;
use sov_stf_runner::{from_toml_path, RollupConfig};
use sqlx::{Pool, Postgres};
use tempfile::TempDir;

use crate::db::Db;
use crate::indexer::index_blocks;
use crate::{api_v0, indexer, AppState, AppStateInner, Config};

fn populate_ledger_db() -> LedgerDB {
    let start_height: u64 = 0u64;
    let mut end_height: u64 = 100u64;
    if let Ok(val) = env::var("BLOCKS") {
        end_height = val.parse().expect("BLOCKS var should be a +ve number");
    }

    let mut rollup_config: RollupConfig<sov_celestia_adapter::DaServiceConfig> =
        toml::from_str(include_str!("rollup_config.toml"))
            .expect("Failed to read rollup configuration");

    let temp_dir = TempDir::new().expect("Unable to create temporary directory");
    rollup_config.storage.path = PathBuf::from(temp_dir.path());
    let ledger_db =
        LedgerDB::with_path(&rollup_config.storage.path).expect("Ledger DB failed to open");

    let da_service = Arc::new(RngDaService::default());

    let demo_runner = App::<Risc0Verifier, RngDaSpec>::new(rollup_config.storage);

    let mut demo = demo_runner.stf;
    let sequencer_private_key = DefaultPrivateKey::generate();
    let sequencer_da_address = MockAddress::from(RngDaService::SEQUENCER_DA_ADDRESS);
    let demo_genesis_config = create_demo_genesis_config(
        100_000_000,
        sequencer_private_key.default_address(),
        sequencer_da_address.as_ref().to_vec(),
        &sequencer_private_key,
        #[cfg(feature = "experimental")]
        Default::default(),
    );

    demo.init_chain(demo_genesis_config);

    // data generation
    let mut blobs = vec![];
    let mut blocks = vec![];
    for height in start_height..end_height {
        println!("Generating block {}", height);
        let num_bytes = height.to_le_bytes();
        let mut barray = [0u8; 32];
        barray[..num_bytes.len()].copy_from_slice(&num_bytes);
        let filtered_block = MockBlock {
            header: MockBlockHeader {
                hash: barray.into(),
                prev_hash: [0u8; 32].into(),
                height,
            },
            validity_cond: Default::default(),
            blobs: Default::default(),
        };
        blocks.push(filtered_block.clone());

        let blob_txs = da_service.extract_relevant_txs(&filtered_block);
        blobs.push(blob_txs.clone());
    }

    let mut height = 0u64;
    while height < end_height {
        println!("Processing block {}", height);
        let filtered_block = &blocks[height as usize];

        let mut data_to_commit = SlotCommit::new(filtered_block.clone());
        let apply_block_result = demo.apply_slot(
            Default::default(),
            &filtered_block.header,
            &filtered_block.validity_cond,
            &mut blobs[height as usize],
        );
        for receipts in apply_block_result.batch_receipts {
            data_to_commit.add_batch(receipts);
        }

        ledger_db.commit_slot(data_to_commit).unwrap();
        height += 1;
    }

    ledger_db
}

async fn create_test_server(pool: Pool<Postgres>) -> TestServer {
    let ledger_db = populate_ledger_db();
    let app_state = Arc::new(AppStateInner {
        db: Db { pool },
        rpc: ledger_db,
        base_url: "http://localhost:3010".to_string(),
    });
    index_blocks(app_state.clone(), Duration::default()).await;
    let service = crate::api_v0::router(app_state).into_make_service();
    TestServer::new(service).unwrap()
}

#[sqlx::test]
async fn test(pool: Pool<Postgres>) {
    let server = create_test_server(pool).await;
    let txs = server
        .get("/transactions")
        .await
        .json::<serde_json::Value>();
    assert_eq!(txs["data"].as_array().unwrap().len(), 25);
}