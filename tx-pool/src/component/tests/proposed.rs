use crate::component::tests::util::{
    build_tx, build_tx_with_dep, build_tx_with_header_dep, DEFAULT_MAX_ANCESTORS_COUNT,
    MOCK_CYCLES, MOCK_FEE, MOCK_SIZE,
};
use crate::component::{entry::TxEntry, proposed::ProposedPool};
use ckb_types::{
    bytes::Bytes,
    core::{
        cell::{get_related_dep_out_points, CellMeta, ResolvedTransaction},
        Capacity, DepType, TransactionBuilder, TransactionView,
    },
    h256,
    packed::{Byte32, CellDep, CellInput, CellOutput, OutPoint},
    prelude::*,
};
use std::collections::HashSet;
use std::sync::Arc;

fn dummy_resolve<F: Fn(&OutPoint) -> Option<Bytes>>(
    tx: TransactionView,
    get_cell_data: F,
) -> Arc<ResolvedTransaction> {
    let resolved_cell_deps = get_related_dep_out_points(&tx, get_cell_data)
        .expect("dummy resolve")
        .into_iter()
        .map(|out_point| {
            CellMeta {
                cell_output: CellOutput::new_builder().build(),
                out_point,
                transaction_info: None,
                data_bytes: 0,
                mem_cell_data: None,
                mem_cell_data_hash: None, // make sure load_cell_data_hash works within block
            }
        })
        .collect();

    Arc::new(ResolvedTransaction {
        transaction: tx,
        resolved_cell_deps,
        resolved_inputs: vec![],
        resolved_dep_groups: vec![],
    })
}

#[test]
fn test_add_entry() {
    let tx1 = build_tx(vec![(&Byte32::zero(), 1), (&Byte32::zero(), 2)], 1);
    let tx1_hash = tx1.hash();
    let tx2 = build_tx(vec![(&tx1_hash, 0)], 1);

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);

    pool.add_entry(TxEntry::new(
        dummy_resolve(tx1.clone(), |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    ))
    .unwrap();
    pool.add_entry(TxEntry::new(
        dummy_resolve(tx2, |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    ))
    .unwrap();

    assert_eq!(pool.size(), 2);
    assert_eq!(pool.edges.outputs_len(), 2);
    assert_eq!(pool.edges.inputs_len(), 3);

    pool.remove_committed_tx(&tx1);
    assert_eq!(pool.edges.outputs_len(), 1);
    assert_eq!(pool.edges.inputs_len(), 1);
}

#[test]
fn test_add_entry_from_detached() {
    let tx1 = build_tx(vec![(&Byte32::zero(), 1), (&Byte32::zero(), 2)], 1);
    let tx1_hash = tx1.hash();
    let tx2 = build_tx(vec![(&tx1_hash, 0)], 1);
    let tx2_hash = tx2.hash();
    let tx3 = build_tx_with_dep(vec![(&Byte32::zero(), 0)], vec![(&tx2_hash, 0)], 1);

    let entry1 = TxEntry::new(dummy_resolve(tx1.clone(), |_| None), 1, MOCK_FEE, 1);
    let entry2 = TxEntry::new(dummy_resolve(tx2, |_| None), 1, MOCK_FEE, 1);
    let entry3 = TxEntry::new(dummy_resolve(tx3, |_| None), 1, MOCK_FEE, 1);

    let id1 = entry1.proposal_short_id();
    let id2 = entry2.proposal_short_id();
    let id3 = entry3.proposal_short_id();

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);
    pool.add_entry(entry1.clone()).unwrap();
    pool.add_entry(entry2.clone()).unwrap();
    pool.add_entry(entry3).unwrap();

    assert_eq!(pool.size(), 3);
    assert_eq!(pool.edges.outputs_len(), 3);
    assert_eq!(pool.edges.inputs_len(), 4);

    assert_eq!(pool.inner().sorted_index.len(), 3);

    let expected = vec![(id1.clone(), 1), (id2.clone(), 2), (id3.clone(), 3)];
    for (idx, key) in pool.inner().sorted_index.iter().enumerate() {
        assert_eq!(key.id, expected[idx].0);
        assert_eq!(key.ancestors_size, expected[idx].1);
    }

    // check link
    {
        assert!(pool.inner().links.get_parents(&id1).unwrap().is_empty());
        assert_eq!(
            pool.inner().links.get_children(&id1).unwrap(),
            &HashSet::from_iter(vec![id2.clone()].into_iter())
        );

        assert_eq!(
            pool.inner().links.get_parents(&id2).unwrap(),
            &HashSet::from_iter(vec![id1.clone()].into_iter())
        );
        assert_eq!(
            pool.inner()
                .links
                .get_children(&entry2.proposal_short_id())
                .unwrap(),
            &HashSet::from_iter(vec![id3.clone()].into_iter())
        );

        assert_eq!(
            pool.inner().links.get_parents(&id3).unwrap(),
            &HashSet::from_iter(vec![id2.clone()].into_iter())
        );
        assert!(pool.inner().links.get_children(&id3).unwrap().is_empty());
    }

    pool.remove_committed_tx(&tx1);
    assert_eq!(pool.edges.outputs_len(), 2);
    assert_eq!(pool.edges.inputs_len(), 2);
    assert_eq!(pool.inner().sorted_index.len(), 2);

    let removed_expected = vec![(id2.clone(), 1), (id3.clone(), 2)];
    for (idx, key) in pool.inner().sorted_index.iter().enumerate() {
        assert_eq!(key.id, removed_expected[idx].0);
        assert_eq!(key.ancestors_size, removed_expected[idx].1);
    }
    assert!(pool
        .inner()
        .links
        .get_parents(&entry2.proposal_short_id())
        .unwrap()
        .is_empty());

    assert!(pool.add_entry(entry1).unwrap());
    for (idx, key) in pool.inner().sorted_index.iter().enumerate() {
        assert_eq!(key.id, expected[idx].0);
        assert_eq!(key.ancestors_size, expected[idx].1);
    }
    {
        assert!(pool.inner().links.get_parents(&id1).unwrap().is_empty());
        assert_eq!(
            pool.inner().links.get_children(&id1).unwrap(),
            &HashSet::from_iter(vec![id2.clone()].into_iter())
        );

        assert_eq!(
            pool.inner().links.get_parents(&id2).unwrap(),
            &HashSet::from_iter(vec![id1].into_iter())
        );
        assert_eq!(
            pool.inner()
                .links
                .get_children(&entry2.proposal_short_id())
                .unwrap(),
            &HashSet::from_iter(vec![id3.clone()].into_iter())
        );

        assert_eq!(
            pool.inner().links.get_parents(&id3).unwrap(),
            &HashSet::from_iter(vec![id2].into_iter())
        );
        assert!(pool.inner().links.get_children(&id3).unwrap().is_empty());
    }
}

#[test]
fn test_add_roots() {
    let tx1 = build_tx(vec![(&Byte32::zero(), 1), (&Byte32::zero(), 2)], 1);
    let tx2 = build_tx(
        vec![(&h256!("0x2").pack(), 1), (&h256!("0x3").pack(), 2)],
        3,
    );

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);

    pool.add_entry(TxEntry::new(
        dummy_resolve(tx1.clone(), |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    ))
    .unwrap();
    pool.add_entry(TxEntry::new(
        dummy_resolve(tx2, |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    ))
    .unwrap();

    assert_eq!(pool.edges.outputs_len(), 4);
    assert_eq!(pool.edges.inputs_len(), 4);

    pool.remove_committed_tx(&tx1);

    assert_eq!(pool.edges.outputs_len(), 3);
    assert_eq!(pool.edges.inputs_len(), 2);
}

#[test]
#[allow(clippy::cognitive_complexity)]
fn test_add_no_roots() {
    let tx1 = build_tx(vec![(&Byte32::zero(), 1)], 3);
    let tx2 = build_tx(vec![], 4);
    let tx1_hash = tx1.hash();
    let tx2_hash = tx2.hash();

    let tx3 = build_tx(vec![(&tx1_hash, 0), (&Byte32::zero(), 2)], 2);
    let tx4 = build_tx(vec![(&tx1_hash, 1), (&tx2_hash, 0)], 2);

    let tx3_hash = tx3.hash();
    let tx5 = build_tx(vec![(&tx1_hash, 2), (&tx3_hash, 0)], 2);

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);

    pool.add_entry(TxEntry::new(
        dummy_resolve(tx1.clone(), |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    ))
    .unwrap();
    pool.add_entry(TxEntry::new(
        dummy_resolve(tx2, |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    ))
    .unwrap();
    pool.add_entry(TxEntry::new(
        dummy_resolve(tx3, |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    ))
    .unwrap();
    pool.add_entry(TxEntry::new(
        dummy_resolve(tx4, |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    ))
    .unwrap();
    pool.add_entry(TxEntry::new(
        dummy_resolve(tx5, |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    ))
    .unwrap();

    assert_eq!(pool.edges.outputs_len(), 13);
    assert_eq!(pool.edges.inputs_len(), 7);

    pool.remove_committed_tx(&tx1);

    assert_eq!(pool.edges.outputs_len(), 10);
    assert_eq!(pool.edges.inputs_len(), 6);
}

#[test]
fn test_sorted_by_tx_fee_rate() {
    let tx1 = build_tx(vec![(&Byte32::zero(), 1)], 1);
    let tx2 = build_tx(vec![(&Byte32::zero(), 2)], 1);
    let tx3 = build_tx(vec![(&Byte32::zero(), 3)], 1);

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);

    let cycles = 5_000_000;
    let size = 200;

    pool.add_entry(TxEntry::dummy_resolve(
        tx1.clone(),
        cycles,
        Capacity::shannons(100),
        size,
    ))
    .unwrap();
    pool.add_entry(TxEntry::dummy_resolve(
        tx2.clone(),
        cycles,
        Capacity::shannons(300),
        size,
    ))
    .unwrap();
    pool.add_entry(TxEntry::dummy_resolve(
        tx3.clone(),
        cycles,
        Capacity::shannons(200),
        size,
    ))
    .unwrap();

    let txs_sorted_by_fee_rate = pool
        .score_sorted_iter()
        .map(|entry| entry.transaction().hash())
        .collect::<Vec<_>>();
    let expect_result = vec![tx2.hash(), tx3.hash(), tx1.hash()];
    assert_eq!(txs_sorted_by_fee_rate, expect_result);
}

#[test]
fn test_sorted_by_ancestors_score() {
    let tx1 = build_tx(vec![(&Byte32::zero(), 1)], 2);
    let tx1_hash = tx1.hash();
    let tx2 = build_tx(vec![(&tx1_hash, 1)], 1);
    let tx2_hash = tx2.hash();
    let tx3 = build_tx(vec![(&tx1_hash, 2)], 1);
    let tx4 = build_tx(vec![(&tx2_hash, 1)], 1);

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);

    let cycles = 5_000_000;
    let size = 200;

    pool.add_entry(TxEntry::dummy_resolve(
        tx1.clone(),
        cycles,
        Capacity::shannons(100),
        size,
    ))
    .unwrap();
    pool.add_entry(TxEntry::dummy_resolve(
        tx2.clone(),
        cycles,
        Capacity::shannons(300),
        size,
    ))
    .unwrap();
    pool.add_entry(TxEntry::dummy_resolve(
        tx3.clone(),
        cycles,
        Capacity::shannons(200),
        size,
    ))
    .unwrap();
    pool.add_entry(TxEntry::dummy_resolve(
        tx4.clone(),
        cycles,
        Capacity::shannons(400),
        size,
    ))
    .unwrap();

    let txs_sorted_by_fee_rate = pool
        .score_sorted_iter()
        .map(|entry| entry.transaction().hash())
        .collect::<Vec<_>>();
    let expect_result = vec![tx4.hash(), tx2.hash(), tx3.hash(), tx1.hash()];
    assert_eq!(txs_sorted_by_fee_rate, expect_result);
}

#[test]
fn test_sorted_by_ancestors_score_competitive() {
    let tx1 = build_tx(vec![(&Byte32::zero(), 1)], 2);
    let tx1_hash = tx1.hash();
    let tx2 = build_tx(vec![(&tx1_hash, 0)], 1);
    let tx2_hash = tx2.hash();
    let tx3 = build_tx(vec![(&tx2_hash, 0)], 1);

    let tx2_1 = build_tx(vec![(&Byte32::zero(), 2)], 2);
    let tx2_1_hash = tx2_1.hash();
    let tx2_2 = build_tx(vec![(&tx2_1_hash, 0)], 1);
    let tx2_2_hash = tx2_2.hash();
    let tx2_3 = build_tx(vec![(&tx2_2_hash, 0)], 1);
    let tx2_3_hash = tx2_3.hash();
    let tx2_4 = build_tx(vec![(&tx2_3_hash, 0)], 1);

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);

    // Choose 5_000_839, so the weight is 853.0001094046, which will not lead to carry when
    // calculating the weight for a package.
    let cycles = 5_000_839;
    let size = 200;

    for &tx in &[&tx1, &tx2, &tx3, &tx2_1, &tx2_2, &tx2_3, &tx2_4] {
        pool.add_entry(TxEntry::dummy_resolve(
            tx.clone(),
            cycles,
            Capacity::shannons(200),
            size,
        ))
        .unwrap();
    }

    let txs_sorted_by_fee_rate = pool
        .score_sorted_iter()
        .map(|entry| format!("{}", entry.transaction().hash()))
        .collect::<Vec<_>>();
    // the entry with most ancestors score will win
    let expect_result = format!("{}", tx2_4.hash());
    assert_eq!(txs_sorted_by_fee_rate[0], expect_result);
}

#[test]
fn test_get_ancestors() {
    let tx1 = build_tx(vec![(&Byte32::zero(), 1)], 2);
    let tx1_hash = tx1.hash();
    let tx2 = build_tx(vec![(&tx1_hash, 0)], 1);
    let tx2_hash = tx2.hash();
    let tx3 = build_tx(vec![(&tx1_hash, 1)], 1);
    let tx4 = build_tx(vec![(&tx2_hash, 0)], 1);

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);

    let cycles = 5_000_000;
    let size = 200;

    pool.add_entry(TxEntry::dummy_resolve(
        tx1.clone(),
        cycles,
        Capacity::shannons(100),
        size,
    ))
    .unwrap();
    pool.add_entry(TxEntry::dummy_resolve(
        tx2.clone(),
        cycles,
        Capacity::shannons(300),
        size,
    ))
    .unwrap();
    pool.add_entry(TxEntry::dummy_resolve(
        tx3.clone(),
        cycles,
        Capacity::shannons(200),
        size,
    ))
    .unwrap();
    pool.add_entry(TxEntry::dummy_resolve(
        tx4.clone(),
        cycles,
        Capacity::shannons(400),
        size,
    ))
    .unwrap();

    let ancestors = pool.calc_ancestors(&tx4.proposal_short_id());
    let expect_result = vec![tx1.proposal_short_id(), tx2.proposal_short_id()]
        .into_iter()
        .collect();
    assert_eq!(ancestors, expect_result);
    let entry = pool.get(&tx4.proposal_short_id()).expect("exists");
    assert_eq!(
        entry.ancestors_cycles,
        ancestors
            .iter()
            .map(|id| pool.get(id).unwrap().cycles)
            .sum::<u64>()
            + cycles
    );
    assert_eq!(
        entry.ancestors_size,
        ancestors
            .iter()
            .map(|id| pool.get(id).unwrap().size)
            .sum::<usize>()
            + size
    );
    assert_eq!(entry.ancestors_count, ancestors.len() + 1);

    let ancestors = pool.calc_ancestors(&tx3.proposal_short_id());
    let expect_result = vec![tx1.proposal_short_id()].into_iter().collect();
    assert_eq!(ancestors, expect_result);
    let entry = pool.get(&tx3.proposal_short_id()).expect("exists");
    assert_eq!(
        entry.ancestors_cycles,
        ancestors
            .iter()
            .map(|id| pool.get(id).unwrap().cycles)
            .sum::<u64>()
            + cycles
    );
    assert_eq!(
        entry.ancestors_size,
        ancestors
            .iter()
            .map(|id| pool.get(id).unwrap().size)
            .sum::<usize>()
            + size
    );
    assert_eq!(entry.ancestors_count, ancestors.len() + 1);

    let ancestors = pool.calc_ancestors(&tx1.proposal_short_id());
    assert_eq!(ancestors, Default::default());
    let entry = pool.get(&tx1.proposal_short_id()).expect("exists");
    assert_eq!(entry.ancestors_cycles, cycles);
    assert_eq!(entry.ancestors_size, size);
    assert_eq!(entry.ancestors_count, 1);
}

#[test]
fn test_dep_group() {
    let tx1 = build_tx(vec![(&h256!("0x1").pack(), 0)], 1);
    let tx1_out_point = OutPoint::new(tx1.hash(), 0);

    // Dep group cell
    let tx2_data = vec![tx1_out_point.clone()].pack().as_bytes();
    let tx2 = TransactionBuilder::default()
        .input(CellInput::new(OutPoint::new(h256!("0x2").pack(), 0), 0))
        .output(
            CellOutput::new_builder()
                .capacity(Capacity::bytes(1000).unwrap().pack())
                .build(),
        )
        .output_data(tx2_data.pack())
        .build();
    let tx2_out_point = OutPoint::new(tx2.hash(), 0);

    // Transaction use dep group
    let dep = CellDep::new_builder()
        .out_point(tx2_out_point.clone())
        .dep_type(DepType::DepGroup.into())
        .build();
    let tx3 = TransactionBuilder::default()
        .cell_dep(dep)
        .input(CellInput::new(OutPoint::new(h256!("0x3").pack(), 0), 0))
        .output(
            CellOutput::new_builder()
                .capacity(Capacity::bytes(3).unwrap().pack())
                .build(),
        )
        .output_data(Bytes::new().pack())
        .build();
    let tx3_out_point = OutPoint::new(tx3.hash(), 0);

    let get_cell_data = |out_point: &OutPoint| -> Option<Bytes> {
        if out_point == &tx2_out_point {
            Some(tx2_data.clone())
        } else {
            None
        }
    };

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);
    for tx in &[&tx1, &tx2, &tx3] {
        pool.add_entry(TxEntry::new(
            dummy_resolve((*tx).clone(), get_cell_data),
            MOCK_CYCLES,
            MOCK_FEE,
            MOCK_SIZE,
        ))
        .unwrap();
    }

    let get_deps_len = |pool: &ProposedPool, out_point: &OutPoint| -> usize {
        pool.edges
            .deps
            .get(out_point)
            .map(|deps| deps.len())
            .unwrap_or_default()
    };
    assert_eq!(get_deps_len(&pool, &tx1_out_point), 1);
    assert_eq!(get_deps_len(&pool, &tx2_out_point), 1);
    assert_eq!(get_deps_len(&pool, &tx3_out_point), 0);

    pool.remove_committed_tx(&tx3);
    assert_eq!(get_deps_len(&pool, &tx1_out_point), 0);
    assert_eq!(get_deps_len(&pool, &tx2_out_point), 0);
    assert_eq!(get_deps_len(&pool, &tx3_out_point), 0);
}

#[test]
fn test_resolve_conflict_header_dep() {
    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);

    let header: Byte32 = h256!("0x1").pack();
    let tx = build_tx_with_header_dep(
        vec![(&Byte32::zero(), 1), (&h256!("0x1").pack(), 1)],
        vec![header.clone()],
        1,
    );

    let entry = TxEntry::dummy_resolve(tx, MOCK_CYCLES, MOCK_FEE, MOCK_SIZE);

    assert!(pool.add_entry(entry.clone()).is_ok());

    let mut headers = HashSet::new();
    headers.insert(header);

    let conflicts = pool.resolve_conflict_header_dep(&headers);
    assert_eq!(
        conflicts.into_iter().map(|i| i.0).collect::<HashSet<_>>(),
        HashSet::from_iter(vec![entry])
    );
}

#[test]
fn test_disordered_remove_committed_tx() {
    let tx1 = build_tx(vec![(&Byte32::zero(), 1)], 1);
    let tx1_hash = tx1.hash();
    let tx2 = build_tx(vec![(&tx1_hash, 0)], 1);

    let entry1 = TxEntry::new(
        dummy_resolve(tx1.clone(), |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    );
    let entry2 = TxEntry::new(
        dummy_resolve(tx2.clone(), |_| None),
        MOCK_CYCLES,
        MOCK_FEE,
        MOCK_SIZE,
    );

    let mut pool = ProposedPool::new(DEFAULT_MAX_ANCESTORS_COUNT);

    pool.add_entry(entry1).unwrap();
    pool.add_entry(entry2).unwrap();

    assert_eq!(pool.edges.outputs_len(), 2);
    assert_eq!(pool.edges.inputs_len(), 2);

    pool.remove_committed_tx(&tx2);
    pool.remove_committed_tx(&tx1);

    assert_eq!(pool.edges.inputs_len(), 0);
    assert_eq!(pool.edges.outputs_len(), 0);
}

#[test]
fn test_max_ancestors() {
    let mut pool = ProposedPool::new(1);
    let tx1 = build_tx(vec![(&Byte32::zero(), 0)], 1);
    let tx1_id = tx1.proposal_short_id();
    let tx1_hash = tx1.hash();
    let tx2 = build_tx(vec![(&tx1_hash, 0)], 1);

    let entry1 = TxEntry::dummy_resolve(tx1, MOCK_CYCLES, MOCK_FEE, MOCK_SIZE);
    let entry2 = TxEntry::dummy_resolve(tx2, MOCK_CYCLES, MOCK_FEE, MOCK_SIZE);

    assert!(pool.add_entry(entry1).is_ok());
    assert!(pool.add_entry(entry2).is_err());
    assert_eq!(
        pool.inner()
            .get_children(&tx1_id)
            .map(|children| children.is_empty()),
        Some(true)
    );
    assert!(pool.inner().calc_descendants(&tx1_id).is_empty());

    assert_eq!(pool.edges.inputs_len(), 1);
    assert_eq!(pool.edges.outputs_len(), 1);
}

#[test]
fn test_max_ancestors_with_dep() {
    let mut pool = ProposedPool::new(1);
    let tx1 = build_tx_with_dep(
        vec![(&Byte32::zero(), 0)],
        vec![(&h256!("0x1").pack(), 0)],
        1,
    );
    let tx1_id = tx1.proposal_short_id();
    let tx1_hash = tx1.hash();
    let tx2 = build_tx_with_dep(vec![(&tx1_hash, 0)], vec![(&h256!("0x2").pack(), 0)], 1);
    let entry1 = TxEntry::dummy_resolve(tx1, MOCK_CYCLES, MOCK_FEE, MOCK_SIZE);
    let entry2 = TxEntry::dummy_resolve(tx2, MOCK_CYCLES, MOCK_FEE, MOCK_SIZE);

    assert!(pool.add_entry(entry1).is_ok());
    assert!(pool.add_entry(entry2).is_err());
    assert_eq!(pool.inner().deps().len(), 1);
    assert!(pool
        .inner()
        .deps()
        .contains_key(&OutPoint::new(h256!("0x1").pack(), 0)));
    assert!(pool.inner().calc_descendants(&tx1_id).is_empty());

    assert_eq!(pool.edges.inputs_len(), 1);
    assert_eq!(pool.edges.outputs_len(), 1);
}
