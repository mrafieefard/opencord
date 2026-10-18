//! Lavalink nodes (Phase 3 plan §12): the requests of members who manage
//! the server, and the protocol's messages as the app sees them.

use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use crate::api::types::{
    CoreError, LavalinkBridge, LavalinkEventKind, LavalinkHealth, LavalinkNode,
    LavalinkNodeChanges, LavalinkNodeEvent, LavalinkNodeSettings, LavalinkNodes, LavalinkPriority,
    LavalinkTestProgress, LavalinkTestStep, LavalinkTestStepKind, LavalinkTestStepState,
};
use crate::client::{Client, unexpected};

impl Client {
    /// The nodes; with `watch`, their changes arrive as events until it is
    /// called with `watch` false.
    pub async fn lavalink_nodes(&self, key: &str, watch: bool) -> Result<LavalinkNodes, CoreError> {
        let request = Request::FetchLavalinkNodes(proto::FetchLavalinkNodes { watch });
        nodes(self.request(key, request).await?)
    }

    pub async fn create_lavalink_node(
        &self,
        key: &str,
        settings: LavalinkNodeSettings,
        password: String,
    ) -> Result<LavalinkNodes, CoreError> {
        let request = Request::CreateLavalinkNode(proto::CreateLavalinkNode {
            settings: Some(settings_to_proto(settings)),
            password,
        });
        nodes(self.request(key, request).await?)
    }

    pub async fn update_lavalink_node(
        &self,
        key: &str,
        node_id: i64,
        changes: LavalinkNodeChanges,
    ) -> Result<LavalinkNodes, CoreError> {
        let request = Request::UpdateLavalinkNode(proto::UpdateLavalinkNode {
            node_id,
            settings: changes.settings.map(settings_to_proto),
            password: changes.password,
            draining: changes.draining,
        });
        nodes(self.request(key, request).await?)
    }

    pub async fn delete_lavalink_node(
        &self,
        key: &str,
        node_id: i64,
    ) -> Result<LavalinkNodes, CoreError> {
        let request = Request::DeleteLavalinkNode(proto::DeleteLavalinkNode { node_id });
        nodes(self.request(key, request).await?)
    }

    /// Starts the connection test; its steps arrive as events.
    pub async fn test_lavalink_node(&self, key: &str, node_id: i64) -> Result<(), CoreError> {
        let request = Request::TestLavalinkNode(proto::TestLavalinkNode { node_id });
        match self.request(key, request).await? {
            Response::Ack(_) => Ok(()),
            other => Err(unexpected(&other)),
        }
    }
}

fn nodes(response: Response) -> Result<LavalinkNodes, CoreError> {
    match response {
        Response::LavalinkNodes(nodes) => Ok(nodes_from(*nodes)),
        other => Err(unexpected(&other)),
    }
}

pub(crate) fn nodes_from(nodes: proto::LavalinkNodes) -> LavalinkNodes {
    LavalinkNodes {
        nodes: nodes.nodes.into_iter().map(node_from).collect(),
        bridge: bridge_from(nodes.bridge.unwrap_or_default()),
        events: nodes.events.into_iter().map(event_from).collect(),
        channels_playing: nodes.channels_playing,
    }
}

fn priority_from(priority: i32) -> LavalinkPriority {
    match proto::LavalinkPriority::try_from(priority) {
        Ok(proto::LavalinkPriority::Preferred) => LavalinkPriority::Preferred,
        Ok(proto::LavalinkPriority::Backup) => LavalinkPriority::Backup,
        _ => LavalinkPriority::Normal,
    }
}

fn settings_from(settings: proto::LavalinkNodeSettings) -> LavalinkNodeSettings {
    LavalinkNodeSettings {
        name: settings.name,
        host: settings.host,
        port: settings.port,
        secure: settings.secure,
        priority: priority_from(settings.priority),
        max_players: settings.max_players,
        strict_address: settings.strict_address,
        enabled: settings.enabled,
    }
}

fn settings_to_proto(settings: LavalinkNodeSettings) -> proto::LavalinkNodeSettings {
    proto::LavalinkNodeSettings {
        name: settings.name,
        host: settings.host,
        port: settings.port,
        secure: settings.secure,
        priority: match settings.priority {
            LavalinkPriority::Preferred => proto::LavalinkPriority::Preferred,
            LavalinkPriority::Normal => proto::LavalinkPriority::Normal,
            LavalinkPriority::Backup => proto::LavalinkPriority::Backup,
        } as i32,
        max_players: settings.max_players,
        strict_address: settings.strict_address,
        enabled: settings.enabled,
    }
}

pub(crate) fn node_from(node: proto::LavalinkNode) -> LavalinkNode {
    LavalinkNode {
        id: node.id,
        settings: settings_from(node.settings.unwrap_or_default()),
        draining: node.draining,
        health: match proto::LavalinkHealth::try_from(node.health) {
            Ok(proto::LavalinkHealth::Healthy) => LavalinkHealth::Healthy,
            Ok(proto::LavalinkHealth::Degraded) => LavalinkHealth::Degraded,
            Ok(proto::LavalinkHealth::Unavailable) => LavalinkHealth::Unavailable,
            Ok(proto::LavalinkHealth::Draining) => LavalinkHealth::Draining,
            Ok(proto::LavalinkHealth::Disabled) => LavalinkHealth::Disabled,
            _ => LavalinkHealth::Connecting,
        },
        reason: node.reason,
        version: node.version,
        sources: node.sources,
        plugins: node.plugins,
        channels: node.channels,
        players: node.players,
        playing_players: node.playing_players,
        cpu_percent: (node.cpu.clamp(0.0, 1.0) * 100.0).round() as u32,
        ping_ms: node.ping_ms,
        last_error: node.last_error,
        connected_since_ms: node.connected_since_ms,
        recovering: node.recovering,
    }
}

fn bridge_from(bridge: proto::LavalinkBridge) -> LavalinkBridge {
    let stats = bridge.stats.unwrap_or_default();
    LavalinkBridge {
        connections: stats.connections,
        packets: stats.packets,
        decrypt_failures: stats.decrypt_failures,
        dropped: stats.dropped,
        jitter_ms: stats.jitter_ms,
        node_bridges: bridge.node_bridges,
        enabled: bridge.enabled,
        url: bridge.url,
        trusted_certificate: bridge.trusted_certificate,
        audio_address: bridge.audio_address,
        audio_port: bridge.audio_port,
        audio_listening: bridge.audio_listening,
        mapping: crate::network::mapping_state(bridge.mapping),
    }
}

pub(crate) fn event_from(event: proto::LavalinkNodeEvent) -> LavalinkNodeEvent {
    LavalinkNodeEvent {
        node_id: event.node_id,
        kind: match proto::LavalinkEventKind::try_from(event.kind) {
            Ok(proto::LavalinkEventKind::Disconnected) => LavalinkEventKind::Disconnected,
            Ok(proto::LavalinkEventKind::Degraded) => LavalinkEventKind::Degraded,
            Ok(proto::LavalinkEventKind::Recovered) => LavalinkEventKind::Recovered,
            Ok(proto::LavalinkEventKind::Moved) => LavalinkEventKind::Moved,
            _ => LavalinkEventKind::Connected,
        },
        text: event.text,
        at_ms: event.at_ms,
    }
}

pub(crate) fn test_progress_from(progress: proto::LavalinkTestProgress) -> LavalinkTestProgress {
    LavalinkTestProgress {
        node_id: progress.node_id,
        steps: progress
            .steps
            .into_iter()
            .filter_map(|step| {
                let kind = match proto::LavalinkTestStepKind::try_from(step.kind).ok()? {
                    proto::LavalinkTestStepKind::Rest => LavalinkTestStepKind::Rest,
                    proto::LavalinkTestStepKind::Websocket => LavalinkTestStepKind::Websocket,
                    proto::LavalinkTestStepKind::Bridge => LavalinkTestStepKind::Bridge,
                    proto::LavalinkTestStepKind::Unspecified => return None,
                };
                let state = match proto::LavalinkTestStepState::try_from(step.state) {
                    Ok(proto::LavalinkTestStepState::Running) => LavalinkTestStepState::Running,
                    Ok(proto::LavalinkTestStepState::Passed) => LavalinkTestStepState::Passed,
                    Ok(proto::LavalinkTestStepState::Failed) => LavalinkTestStepState::Failed,
                    Ok(proto::LavalinkTestStepState::Skipped) => LavalinkTestStepState::Skipped,
                    _ => LavalinkTestStepState::Waiting,
                };
                Some(LavalinkTestStep {
                    kind,
                    state,
                    detail: step.detail,
                })
            })
            .collect(),
        done: progress.done,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let settings = LavalinkNodeSettings {
            name: "Home".to_owned(),
            host: "lava.example.com".to_owned(),
            port: 443,
            secure: true,
            priority: LavalinkPriority::Backup,
            max_players: 5,
            strict_address: false,
            enabled: true,
        };
        assert_eq!(settings_from(settings_to_proto(settings.clone())), settings);
    }

    #[test]
    fn unknown_test_steps_are_dropped() {
        let progress = test_progress_from(proto::LavalinkTestProgress {
            node_id: 1,
            steps: vec![
                proto::LavalinkTestStep {
                    kind: proto::LavalinkTestStepKind::Rest as i32,
                    state: proto::LavalinkTestStepState::Failed as i32,
                    detail: "Lavalink refused the password".to_owned(),
                },
                proto::LavalinkTestStep {
                    kind: 99,
                    state: 1,
                    detail: String::new(),
                },
            ],
            done: true,
        });
        assert_eq!(progress.steps.len(), 1);
        assert_eq!(progress.steps[0].state, LavalinkTestStepState::Failed);
    }
}
