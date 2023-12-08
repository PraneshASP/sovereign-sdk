use sov_modules_api::default_context::{DefaultContext, ZkDefaultContext};
use sov_modules_api::{Address, Context, Event, Module, WorkingSet};
use sov_state::{ProverStorage, ZkStorage};

use super::OrderModule;
use crate::{call, query, OrderModuleConfig};

#[test]
fn test_submit_order() {
    let tmpdir = tempfile::tempdir().unwrap();
    let mut working_set = WorkingSet::new(ProverStorage::with_path(tmpdir.path()).unwrap());
    let admin = Address::from([1; 32]);
    // Test Native-Context
    #[cfg(feature = "native")]
    {
        let config = OrderModuleConfig { admin };
        let context = DefaultContext::new(admin, 1);
        test_submit_order_helper(context, &config, &mut working_set);
    }

    let (_, witness) = working_set.checkpoint().freeze();

    // Test Zk-Context
    {
        let config = OrderModuleConfig { admin };
        let zk_context = ZkDefaultContext::new(admin, 1);
        let mut zk_working_set = WorkingSet::with_witness(ZkStorage::new(), witness);
        test_submit_order_helper(zk_context, &config, &mut zk_working_set);
    }
}

fn test_submit_order_helper<C: Context>(
    context: C,
    config: &OrderModuleConfig<C>,
    working_set: &mut WorkingSet<C>,
) {
    let module = OrderModule::<C>::default();
    module.genesis(config, working_set).unwrap();

    let call_msg = call::CallMessage::NewMarketOrder {
        order_asset: String::from("USDC"),
        price_asset: String::from("ETH"),
        side: 2,
        qty: 1,
        ts: 1702012020,
    };
    
    module.call(call_msg, &context, working_set).unwrap();
    
    // Test events
    // {
    //     let event = &working_set.events()[0];
    //     assert_eq!(event, &Event::new("set", "order_set: {call_msg:?}"));
    // }

    // Test query
    {
        let id = 12345678;
        let query_response = module.query_order(id, working_set).unwrap();

        let call_msg_expected = call::CallMessage::NewMarketOrder {
            order_asset: String::from("USDC"),
            price_asset: String::from("ETH"),
            side: 2,
            qty: 1,
            ts: 1702012020,
        };

        assert_eq!(
            query::Response {
                order: call_msg_expected
            },
            query_response
        )
    }
}