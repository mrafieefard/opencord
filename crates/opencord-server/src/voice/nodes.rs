//! The voice nodes this server sends people to, and which node serves which
//! channel (Phase 2 plan §3.4). Everyone in a channel is on the same node;
//! a channel keeps its node until it is empty.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use opencord_common::address::Fingerprint;
use opencord_voice::node::{NodeCommand, VoiceNode};

/// Which node serves a channel. Ordered so the embedded node comes first.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NodeId {
    Embedded,
    External(String),
}

/// How clients reach a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeLink {
    /// `wss://` URL; empty for the embedded node, at this server's `/voice`.
    pub endpoint: String,
    pub fingerprint: Fingerprint,
}

/// Sends commands to an external node over its control channel.
pub trait CommandSink: Send + Sync + std::fmt::Debug {
    fn send(&self, command: NodeCommand);
}

/// One control channel's hold on an external node. A node that registers
/// again replaces it; the old one then no longer counts for anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    id: String,
    generation: u64,
}

impl Registration {
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug)]
struct External {
    generation: u64,
    link: NodeLink,
    sink: Box<dyn CommandSink>,
    participants: u32,
}

#[derive(Debug, Default)]
struct Inner {
    external: HashMap<String, External>,
    assignments: HashMap<i64, NodeId>,
    next_generation: u64,
}

impl Inner {
    fn current(&self, registration: &Registration) -> Option<&External> {
        self.external
            .get(&registration.id)
            .filter(|node| node.generation == registration.generation)
    }

    /// Unassigns the channels of `id`, returning them.
    fn take_channels(&mut self, id: &str) -> Vec<i64> {
        let lost: Vec<i64> = self
            .assignments
            .iter()
            .filter(|(_, node)| matches!(node, NodeId::External(node) if node == id))
            .map(|(channel, _)| *channel)
            .collect();
        for channel in &lost {
            self.assignments.remove(channel);
        }
        lost
    }

    fn channels_on(&self, id: &NodeId) -> usize {
        self.assignments.values().filter(|node| *node == id).count()
    }
}

pub struct VoiceNodes {
    embedded: Option<VoiceNode>,
    /// This server's own certificate, which the embedded node is behind.
    fingerprint: Fingerprint,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for VoiceNodes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoiceNodes")
            .field("embedded", &self.embedded.is_some())
            .finish_non_exhaustive()
    }
}

impl VoiceNodes {
    pub fn new(embedded: Option<VoiceNode>, fingerprint: Fingerprint) -> Self {
        Self {
            embedded,
            fingerprint,
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn embedded(&self) -> Option<&VoiceNode> {
        self.embedded.as_ref()
    }

    /// The node serving `channel_id`, picking one if the channel has none:
    /// the node with the fewest people, then the fewest channels; ties go
    /// to the embedded node.
    pub fn link_for(&self, channel_id: i64) -> Option<NodeLink> {
        let mut inner = self.lock();
        let id = match inner.assignments.get(&channel_id).cloned() {
            Some(id) => id,
            None => {
                let embedded = self
                    .embedded
                    .as_ref()
                    .map(|node| (node.load().participants, NodeId::Embedded));
                let external = inner
                    .external
                    .iter()
                    .map(|(id, node)| (node.participants, NodeId::External(id.clone())));
                let (_, _, id) = embedded
                    .into_iter()
                    .chain(external)
                    .map(|(participants, id)| (participants, inner.channels_on(&id), id))
                    .min()?;
                inner.assignments.insert(channel_id, id.clone());
                id
            }
        };
        match id {
            NodeId::Embedded => Some(NodeLink {
                endpoint: String::new(),
                fingerprint: self.fingerprint,
            }),
            NodeId::External(id) => inner.external.get(&id).map(|node| node.link.clone()),
        }
    }

    /// Sends a command to the node serving `channel_id`, if any.
    pub fn send(&self, channel_id: i64, command: NodeCommand) {
        let inner = self.lock();
        match inner.assignments.get(&channel_id) {
            Some(NodeId::Embedded) => {
                if let Some(node) = &self.embedded {
                    node.send(command);
                }
            }
            Some(NodeId::External(id)) => {
                if let Some(node) = inner.external.get(id) {
                    node.sink.send(command);
                }
            }
            None => {}
        }
    }

    /// The channel is empty: its next participant may get another node.
    pub fn release(&self, channel_id: i64) {
        self.lock().assignments.remove(&channel_id);
    }

    /// Channels served now.
    pub fn assigned(&self) -> Vec<i64> {
        self.lock().assignments.keys().copied().collect()
    }

    /// An external node is available. If it was registered already, the old
    /// registration ends and its channels have no node now: the node starts
    /// again without any.
    pub fn register(&self, id: &str, link: NodeLink, sink: Box<dyn CommandSink>) -> Registration {
        let mut inner = self.lock();
        inner.take_channels(id);
        inner.next_generation += 1;
        let generation = inner.next_generation;
        inner.external.insert(
            id.to_owned(),
            External {
                generation,
                link,
                sink,
                participants: 0,
            },
        );
        Registration {
            id: id.to_owned(),
            generation,
        }
    }

    pub fn report_load(&self, registration: &Registration, participants: u32) {
        let mut inner = self.lock();
        if let Some(node) = inner
            .external
            .get_mut(&registration.id)
            .filter(|node| node.generation == registration.generation)
        {
            node.participants = participants;
        }
    }

    /// Whether the registered node serves `channel_id`.
    pub fn serves(&self, registration: &Registration, channel_id: i64) -> bool {
        let inner = self.lock();
        inner.current(registration).is_some()
            && inner.assignments.get(&channel_id)
                == Some(&NodeId::External(registration.id.clone()))
    }

    /// An external node is gone: returns the channels it served, which have
    /// no node now. Nothing happens if it registered again since.
    pub fn unregister(&self, registration: &Registration) -> Vec<i64> {
        let mut inner = self.lock();
        if inner.current(registration).is_none() {
            return Vec::new();
        }
        inner.external.remove(&registration.id);
        inner.take_channels(&registration.id)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use ed25519_dalek::SigningKey;
    use opencord_voice::node::NodeConfig;

    use super::*;

    #[derive(Debug, Default)]
    struct Recorder(Arc<Mutex<Vec<NodeCommand>>>);

    impl CommandSink for Recorder {
        fn send(&self, command: NodeCommand) {
            self.0.lock().unwrap().push(command);
        }
    }

    fn link(name: &str) -> NodeLink {
        NodeLink {
            endpoint: format!("wss://{name}.example:7712"),
            fingerprint: [1; 32],
        }
    }

    fn register(nodes: &VoiceNodes, name: &str) -> Registration {
        nodes.register(name, link(name), Box::new(Recorder::default()))
    }

    #[test]
    fn without_any_node_a_channel_gets_none() {
        let nodes = VoiceNodes::new(None, [0; 32]);

        assert_eq!(nodes.link_for(5), None);
        assert!(nodes.assigned().is_empty());
    }

    #[test]
    fn channels_go_to_the_least_loaded_node_and_stay_until_released() {
        let nodes = VoiceNodes::new(None, [0; 32]);
        let a = register(&nodes, "a");
        let b = register(&nodes, "b");
        nodes.report_load(&a, 10);
        nodes.report_load(&b, 3);

        let first = nodes.link_for(5).unwrap();
        nodes.report_load(&b, 50);
        let again = nodes.link_for(5).unwrap();
        nodes.release(5);
        let after_release = nodes.link_for(5).unwrap();

        assert_eq!(first, link("b"));
        assert_eq!(again, link("b"), "a channel keeps its node");
        assert_eq!(after_release, link("a"));
    }

    #[test]
    fn equally_loaded_nodes_take_turns() {
        let nodes = VoiceNodes::new(None, [0; 32]);
        register(&nodes, "a");
        register(&nodes, "b");

        let picks = [nodes.link_for(1), nodes.link_for(2), nodes.link_for(3)];

        assert_eq!(picks, [Some(link("a")), Some(link("b")), Some(link("a"))]);
    }

    #[tokio::test]
    async fn the_embedded_node_competes_on_load_too() {
        let key = SigningKey::from_bytes(&[3; 32]);
        let (embedded, _events) = VoiceNode::start(NodeConfig {
            udp_port: 0,
            public_address: None,
            verifying_key: Some(key.verifying_key()),
            heartbeat_interval: Duration::from_secs(5),
        })
        .await
        .unwrap();
        let nodes = VoiceNodes::new(Some(embedded), [0; 32]);
        let a = register(&nodes, "a");

        let first = nodes.link_for(1).unwrap();
        let second = nodes.link_for(2).unwrap();
        nodes.report_load(&a, 4);
        let third = nodes.link_for(3).unwrap();

        assert_eq!(first.endpoint, "", "ties go to the embedded node");
        assert_eq!(second, link("a"));
        assert_eq!(third.endpoint, "");
    }

    #[test]
    fn a_lost_node_gives_back_its_channels() {
        let nodes = VoiceNodes::new(None, [0; 32]);
        let commands = Arc::new(Mutex::new(Vec::new()));
        let a = nodes.register("a", link("a"), Box::new(Recorder(Arc::clone(&commands))));
        nodes.link_for(5);
        nodes.link_for(6);
        nodes.send(
            5,
            NodeCommand::Disconnect {
                user_id: 1,
                channel_id: 5,
            },
        );

        let mut lost = nodes.unregister(&a);
        lost.sort();

        assert_eq!(lost, [5, 6]);
        assert_eq!(commands.lock().unwrap().len(), 1);
        assert_eq!(nodes.link_for(5), None);
    }

    #[test]
    fn a_node_registering_again_starts_without_channels() {
        let nodes = VoiceNodes::new(None, [0; 32]);
        let old = register(&nodes, "a");
        nodes.link_for(5);

        let new = register(&nodes, "a");
        let assigned_after = nodes.assigned();
        let lost_by_old = nodes.unregister(&old);

        assert!(assigned_after.is_empty());
        assert!(
            lost_by_old.is_empty(),
            "the old registration is already gone"
        );
        assert_eq!(nodes.link_for(5), Some(link("a")));
        assert!(nodes.serves(&new, 5));
    }

    #[test]
    fn a_node_serves_only_its_own_channels() {
        let nodes = VoiceNodes::new(None, [0; 32]);
        let a = register(&nodes, "a");
        nodes.link_for(5);
        let b = register(&nodes, "b");
        nodes.link_for(6);

        assert!(nodes.serves(&a, 5));
        assert!(!nodes.serves(&a, 6));
        assert!(nodes.serves(&b, 6));
        assert!(!nodes.serves(&b, 7));
    }
}
