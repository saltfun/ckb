use crate::util::mining::out_ibd_mode;
use crate::utils::wait_until;
use crate::{Net, Node, Spec};
use ckb_constant::sync::MAX_LOCATOR_SIZE;
use ckb_logger::info;
use ckb_network::SupportProtocols;
use ckb_types::{
    h256,
    packed::{Byte32, GetHeaders, SyncMessage},
    prelude::*,
};

pub struct InvalidLocatorSize;

impl Spec for InvalidLocatorSize {
    fn run(&self, nodes: &mut Vec<Node>) {
        info!("Connect node0");
        out_ibd_mode(nodes);
        let node0 = &nodes[0];
        let mut net = Net::new(self.name(), node0.consensus(), vec![SupportProtocols::Sync]);
        net.connect(node0);

        let hashes: Vec<Byte32> = (0..=MAX_LOCATOR_SIZE)
            .map(|_| h256!("0x1").pack())
            .collect();

        let message = SyncMessage::new_builder()
            .set(
                GetHeaders::new_builder()
                    .block_locator_hashes(hashes.pack())
                    .build(),
            )
            .build()
            .as_bytes();

        net.send(node0, SupportProtocols::Sync, message);

        let rpc_client = nodes[0].rpc_client();
        let ret = wait_until(10, || rpc_client.get_peers().is_empty());
        assert!(ret, "Node0 should disconnect test node");

        let ret = wait_until(10, || {
            !net.controller()
                .connected_peers()
                .iter()
                .any(|(_, peer)| peer.connected_addr.to_string() == node0.p2p_address())
        });
        assert!(
            ret,
            "Net should disconnect node0 because node0 already disconnect it"
        );

        net.connect_uncheck(node0);
        let ret = wait_until(10, || !rpc_client.get_peers().is_empty());
        assert!(!ret, "Node0 should ban test node");
    }
}
