//! The permissions a plugin can ask for (spec §8.1). Each one's sentence is
//! what the install review shows beside its switch.

/// Every plugin permission and what it allows, in the order the review
/// lists them.
pub const PERMISSIONS: &[(&str, &str)] = &[
    ("read_messages", "Read messages and their history."),
    ("send_messages", "Send messages and reply to commands."),
    (
        "manage_messages",
        "Edit its own messages and delete anyone's.",
    ),
    ("attach_files", "Send files from its storage."),
    ("read_channels", "See channels and changes to them."),
    (
        "manage_channels",
        "Create, edit, delete and reorder channels.",
    ),
    ("create_channel_types", "Register channel types."),
    ("register_activities", "Register activities."),
    ("register_commands", "Register /commands."),
    (
        "read_members",
        "See members, who is online, and changes to them.",
    ),
    ("read_roles", "See roles and changes to them."),
    ("manage_roles", "Manage roles and members' roles."),
    ("manage_nicknames", "Change nicknames."),
    ("kick_members", "Kick members."),
    ("ban_members", "Ban and unban members, and list bans."),
    ("read_invites", "See invites and changes to them."),
    ("create_invites", "Create invites."),
    (
        "read_server",
        "See the server's settings, voice and upload settings included, and changes to them.",
    ),
    (
        "read_voice",
        "See who is in calls, their streams and soundboard sounds.",
    ),
    (
        "move_members",
        "Move members between calls and disconnect them.",
    ),
    ("mute_members", "Server mute members."),
    ("deafen_members", "Server deafen members."),
    (
        "play_soundboard",
        "Play soundboard sounds in a call without being in it.",
    ),
    ("read_music", "See what music is playing and the queue."),
    (
        "control_music",
        "Add, skip and pause music without being in the call.",
    ),
    (
        "internet",
        "Reach the websites its manifest lists, and show images from them.",
    ),
    (
        "receive_web_requests",
        "Answer web requests sent to its own address on this server.",
    ),
    (
        "accept_uploads",
        "Let people upload images and files to it.",
    ),
    (
        "talk_to_plugins",
        "Send events to other plugins and receive the ones they share.",
    ),
];

/// Whether `name` is a plugin permission.
pub fn is_known(name: &str) -> bool {
    PERMISSIONS.iter().any(|(known, _)| *known == name)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_permission_has_a_sentence_and_a_unique_name() {
        let mut names = HashSet::new();
        for (name, sentence) in PERMISSIONS {
            assert!(names.insert(*name), "{name} is listed twice");
            assert!(
                name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{name}"
            );
            assert!(sentence.ends_with('.'), "{name}: {sentence}");
            assert!(
                sentence.starts_with(|c: char| c.is_ascii_uppercase()),
                "{name}: {sentence}"
            );
        }
        // §8.1's table, its mute / deafen row counted as two.
        assert_eq!(PERMISSIONS.len(), 29);
    }

    #[test]
    fn known_names_are_exactly_the_list() {
        assert!(is_known("read_messages"));
        assert!(is_known("talk_to_plugins"));
        assert!(!is_known("send_mesages"));
        assert!(!is_known("mention_everyone"));
        assert!(!is_known("READ_MESSAGES"));
        assert!(!is_known(""));
    }
}
