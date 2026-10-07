//! The voice nodes this server sends people to, and which node serves which
//! channel (Phase 2 plan §3.4). Everyone in a channel is on the same node;
//! a channel keeps its node until it is empty.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use opencord_common::address::Fingerprint;
use opencord_voice::node::{NodeCommand, VoiceNode};

/// Which node serves a channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug)]
struct External {
    link: NodeLink,
    sink: Box<dyn CommandSink>,
    participants: u32,
}

#[derive(Debug, Default)]
struct Inner {
    external: HashMap<String, External>,
    assignments: HashMap<i64, NodeId>,
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
    /// the embedded node, or else the external node with the fewest people.
    pub fn link_for(&self, channel_id: i64) -> Option<NodeLink> {
        let mut inner = self.lock();
        let assigned = inner.assignments.get(&channel_id).cloned();
        let id = match assigned {
            Some(id) => id,
            None => {
                let id = if self.embedded.is_some() {
                    NodeId::Embedded
                } else {
                    let least_loaded = inner
                        .external
                        .iter()
                        .min_by_key(|(_, node)| node.participants)
                        .map(|(id, _)| id.clone())?;
                    NodeId::External(least_loaded)
                };
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

    /// Channels served now, with their node.
    pub fn assigned(&self) -> Vec<i64> {
        self.lock().assignments.keys().copied().collect()
    }

    /// An external node is available.
    pub fn register(&self, id: String, link: NodeLink, sink: Box<dyn CommandSink>) {
        self.lock().external.insert(
            id,
            External {
                link,
                sink,
                participants: 0,
            },
        );
    }

    pub fn report_load(&self, id: &str, participants: u32) {
        if let Some(node) = self.lock().external.get_mut(id) {
            node.participants = participants;
        }
    }

    /// An external node is gone: returns the channels it served, which have
    /// no node now.
    pub fn unregister(&self, id: &str) -> Vec<i64> {
        let mut inner = self.lock();
        inner.external.remove(id);
        let lost: Vec<i64> = inner
            .assignments
            .iter()
            .filter(|(_, node)| matches!(node, NodeId::External(node) if node == id))
            .map(|(channel, _)| *channel)
            .collect();
        for channel in &lost {
            inner.assignments.remove(channel);
        }
        lost
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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

    #[test]
    fn without_any_node_a_channel_gets_none() {
        let nodes = VoiceNodes::new(None, [0; 32]);

        assert_eq!(nodes.link_for(5), None);
        assert!(nodes.assigned().is_empty());
    }

    #[test]
    fn channels_go_to_the_least_loaded_node_and_stay_until_released() {
        let nodes = VoiceNodes::new(None, [0; 32]);
        nodes.register("a".into(), link("a"), Box::new(Recorder::default()));
        nodes.register("b".into(), link("b"), Box::new(Recorder::default()));
        nodes.report_load("a", 10);
        nodes.report_load("b", 3);

        let first = nodes.link_for(5).unwrap();
        nodes.report_load("b", 50);
        let again = nodes.link_for(5).unwrap();
        nodes.release(5);
        let after_release = nodes.link_for(5).unwrap();

        assert_eq!(first, link("b"));
        assert_eq!(again, link("b"), "a channel keeps its node");
        assert_eq!(after_release, link("a"));
    }

    #[test]
    fn a_lost_node_gives_back_its_channels() {
        let nodes = VoiceNodes::new(None, [0; 32]);
        let commands = Arc::new(Mutex::new(Vec::new()));
        nodes.register(
            "a".into(),
            link("a"),
            Box::new(Recorder(Arc::clone(&commands))),
        );
        nodes.link_for(5);
        nodes.link_for(6);
        nodes.send(
            5,
            NodeCommand::Disconnect {
                user_id: 1,
                channel_id: 5,
            },
        );

        let mut lost = nodes.unregister("a");
        lost.sort();

        assert_eq!(lost, [5, 6]);
        assert_eq!(commands.lock().unwrap().len(), 1);
        assert_eq!(nodes.link_for(5), None);
    }
}
