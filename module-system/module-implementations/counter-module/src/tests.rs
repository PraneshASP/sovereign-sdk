use sov_modules_api::default_context::{DefaultContext, ZkDefaultContext};
use sov_modules_api::{Address, Context, Event, Module, WorkingSet};
use sov_state::{ProverStorage, ZkStorage};

use super::CounterModule;
use crate::{call, query, CounterModuleConfig};

#[test]
fn test_value_setter() {
    let tmpdir = tempfile::tempdir().unwrap();
    let mut working_set = WorkingSet::new(ProverStorage::with_path(tmpdir.path()).unwrap());
    let admin = Address::from([1; 32]);
    // Test Native-Context
    #[cfg(feature = "native")]
    {
        let config = CounterModuleConfig { admin };
        let context = DefaultContext::new(admin, 1);
        test_value_setter_helper(context, &config, &mut working_set);
    }

    let (_, witness) = working_set.checkpoint().freeze();

    // Test Zk-Context
    {
        let config = CounterModuleConfig { admin };
        let zk_context = ZkDefaultContext::new(admin, 1);
        let mut zk_working_set = WorkingSet::with_witness(ZkStorage::new(), witness);
        test_value_setter_helper(zk_context, &config, &mut zk_working_set);
    }
}

fn test_value_setter_helper<C: Context>(
    context: C,
    config: &CounterModuleConfig<C>,
    working_set: &mut WorkingSet<C>,
) {
    let module = CounterModule::<C>::default();
    module.genesis(config, working_set).unwrap();

    let new_value = 99;
    let call_msg = call::CallMessage::SetValue(new_value);

    // Test events
    {
        module.call(call_msg, &context, working_set).unwrap();
        let event = &working_set.events()[0];
        assert_eq!(event, &Event::new("set", "count_set: 99"));
    }

    // Test query
    {
        let query_response = module.query_count(working_set).unwrap();

        assert_eq!(
            query::Response {
                count: Some(new_value)
            },
            query_response
        )
    }
}

#[test]
fn test_err_on_sender_is_not_admin() {
    let sender = Address::from([1; 32]);

    let tmpdir = tempfile::tempdir().unwrap();
    let backing_store = ProverStorage::with_path(tmpdir.path()).unwrap();
    let mut native_working_set = WorkingSet::new(backing_store);

    let sender_not_admin = Address::from([2; 32]);
    // Test Native-Context
    #[cfg(feature = "native")]
    {
        let config = CounterModuleConfig {
            admin: sender_not_admin,
        };
        let context = DefaultContext::new(sender, 1);
        test_err_on_sender_is_not_admin_helper(context, &config, &mut native_working_set);
    }
    let (_, witness) = native_working_set.checkpoint().freeze();

    // Test Zk-Context
    {
        let config = CounterModuleConfig {
            admin: sender_not_admin,
        };
        let zk_backing_store = ZkStorage::new();
        let zk_context = ZkDefaultContext::new(sender, 1);
        let zk_working_set = &mut WorkingSet::with_witness(zk_backing_store, witness);
        test_err_on_sender_is_not_admin_helper(zk_context, &config, zk_working_set);
    }
}

fn test_err_on_sender_is_not_admin_helper<C: Context>(
    context: C,
    config: &CounterModuleConfig<C>,
    working_set: &mut WorkingSet<C>,
) {
    let module = CounterModule::<C>::default();
    module.genesis(config, working_set).unwrap();
    let resp = module.set_value(11, &context, working_set);

    assert!(resp.is_err());
}

#[test]
fn test_increment() {
    let tmpdir = tempfile::tempdir().unwrap();
    let mut working_set = WorkingSet::new(ProverStorage::with_path(tmpdir.path()).unwrap());
    let admin = Address::from([1; 32]);
    // Test Native-Context
    #[cfg(feature = "native")]
    {
        let config = CounterModuleConfig { admin };
        let context = DefaultContext::new(admin, 1);
        test_value_setter_helper(context, &config, &mut working_set);
    }

    let (_, witness) = working_set.checkpoint().freeze();

    // Test Zk-Context
    {
        let config = CounterModuleConfig { admin };
        let zk_context = ZkDefaultContext::new(admin, 1);
        let mut zk_working_set = WorkingSet::with_witness(ZkStorage::new(), witness);
        test_increment_helper(zk_context, &config, &mut zk_working_set);
    }
}

fn test_increment_helper<C: Context>(
    context: C,
    config: &CounterModuleConfig<C>,
    working_set: &mut WorkingSet<C>,
) {
    let module = CounterModule::<C>::default();
    module.genesis(config, working_set).unwrap();

    let new_value: u32 = 99;
    let set_call_msg = call::CallMessage::SetValue(new_value);
    let increment_call_msg = call::CallMessage::Increment;

    // Test events
    {
        module.call(set_call_msg, &context, working_set).unwrap();
        module
            .call(increment_call_msg, &context, working_set)
            .unwrap();

        let event = &working_set.events()[1];
        assert_eq!(event, &Event::new("increment", "count_incremented: 100"));
    }

    // Test query
    {
        let query_response = module.query_count(working_set).unwrap();

        assert_eq!(query::Response { count: Some(100) }, query_response)
    }
}
