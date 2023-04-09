use super::helper::new_index_transaction;
use crate::relayer::block_transactions_verifier::BlockTransactionsVerifier;
use crate::{Status, StatusCode};
use ckb_types::packed::{CompactBlock, CompactBlockBuilder};
use ckb_types::prelude::*;

// block_short_ids: vec![None, Some(1), None, Some(3), Some(4), None]
fn build_compact_block() -> CompactBlock {
    let prefilled_iter = vec![0, 2, 5].into_iter().map(new_index_transaction);

    let short_ids = vec![1, 3, 4]
        .into_iter()
        .map(new_index_transaction)
        .map(|tx| tx.transaction().proposal_short_id());

    CompactBlockBuilder::default()
        .short_ids(short_ids.pack())
        .prefilled_transactions(prefilled_iter.pack())
        .build()
}

#[test]
fn test_invalid() {
    let block = build_compact_block();
    let indexes = vec![1, 3, 4];

    // Invalid len
    let block_txs: Vec<_> = vec![1, 3]
        .into_iter()
        .map(|i| new_index_transaction(i).transaction().into_view())
        .collect();

    assert_eq!(
        BlockTransactionsVerifier::verify(&block, &indexes, block_txs.as_slice()),
        StatusCode::BlockTransactionsLengthIsUnmatchedWithPendingCompactBlock.into(),
    );

    // Unordered txs
    let block_txs: Vec<_> = vec![1, 4, 3]
        .into_iter()
        .map(|i| new_index_transaction(i).transaction().into_view())
        .collect();
    assert_eq!(
        BlockTransactionsVerifier::verify(&block, &indexes, &block_txs),
        StatusCode::BlockTransactionsShortIdsAreUnmatchedWithPendingCompactBlock.into(),
    );
}

#[test]
fn test_ok() {
    let block = build_compact_block();

    let indexes = vec![1, 3, 4];
    let block_txs: Vec<_> = vec![1, 3, 4]
        .into_iter()
        .map(|i| new_index_transaction(i).transaction().into_view())
        .collect();

    assert_eq!(
        BlockTransactionsVerifier::verify(&block, &indexes, &block_txs),
        Status::ok()
    );
}
