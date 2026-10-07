import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/permissions.dart';

/// A permission as people read it (§8.2), matching the bits in the Phase 1
/// plan §6.1.
@immutable
class PermissionInfo {
  const PermissionInfo(
    this.bit,
    this.label,
    this.description, {
    this.perChannel = true,
    this.later = false,
  });

  final Permissions bit;
  final String label;
  final String description;

  /// Whether a channel can allow or deny it (§8.2 Permissions tab).
  final bool perChannel;

  /// Reserved for a later phase: saved, but nothing uses it yet.
  final bool later;
}

/// The groups the roles editor shows (§8.2): General · Text · Members ·
/// Voice & video · Advanced.
const permissionGroups = <(String, List<PermissionInfo>)>[
  (
    'General',
    [
      PermissionInfo(
        Permissions.viewChannel,
        'View channels',
        'See channels and what is in them',
      ),
      PermissionInfo(
        Permissions.manageChannels,
        'Manage channels',
        'Create, edit, reorder and delete channels',
      ),
      PermissionInfo(
        Permissions.manageRoles,
        'Manage roles',
        'Create and edit roles below their own, and give them out',
        perChannel: false,
      ),
      PermissionInfo(
        Permissions.createInvite,
        'Create invites',
        'Make invite links to bring people in',
      ),
      PermissionInfo(
        Permissions.manageServer,
        'Manage server',
        'Change the name, description and who may join',
        perChannel: false,
      ),
      PermissionInfo(
        Permissions.changeNickname,
        'Change nickname',
        'Pick their own name on this server',
        perChannel: false,
      ),
      PermissionInfo(
        Permissions.manageNicknames,
        'Manage nicknames',
        "Change other members' nicknames",
        perChannel: false,
      ),
    ],
  ),
  (
    'Text',
    [
      PermissionInfo(
        Permissions.sendMessages,
        'Send messages',
        'Write in text channels',
      ),
      PermissionInfo(
        Permissions.readHistory,
        'Read message history',
        'Read what was written before they arrived',
      ),
      PermissionInfo(
        Permissions.manageMessages,
        'Manage messages',
        "Delete and pin other people's messages",
      ),
      PermissionInfo(
        Permissions.attachFiles,
        'Attach files',
        'Send files and images',
        later: true,
      ),
      PermissionInfo(
        Permissions.mentionEveryone,
        'Mention @everyone',
        'Notify everyone who can see the channel at once',
      ),
    ],
  ),
  (
    'Members',
    [
      PermissionInfo(
        Permissions.kickMembers,
        'Kick members',
        'Remove members below their own role; they can come back',
        perChannel: false,
      ),
      PermissionInfo(
        Permissions.banMembers,
        'Ban members',
        'Remove members below their own role for good',
        perChannel: false,
      ),
    ],
  ),
  (
    'Voice & video',
    [
      PermissionInfo(
        Permissions.connect,
        'Connect',
        'Join voice channels',
        later: true,
      ),
      PermissionInfo(
        Permissions.speak,
        'Speak',
        'Talk in voice channels',
        later: true,
      ),
      PermissionInfo(
        Permissions.video,
        'Video',
        'Turn on their camera',
        later: true,
      ),
      PermissionInfo(
        Permissions.screenshare,
        'Share screen',
        'Show their screen in voice channels',
        later: true,
      ),
      PermissionInfo(
        Permissions.muteMembers,
        'Mute members',
        'Mute others for everyone',
        later: true,
      ),
      PermissionInfo(
        Permissions.deafenMembers,
        'Deafen members',
        'Deafen others for everyone',
        later: true,
      ),
      PermissionInfo(
        Permissions.moveMembers,
        'Move members',
        'Move others between voice channels',
        later: true,
      ),
      PermissionInfo(
        Permissions.prioritySpeaker,
        'Priority speaker',
        'Be heard over others',
        later: true,
      ),
    ],
  ),
  (
    'Advanced',
    [
      PermissionInfo(
        Permissions.administrator,
        'Administrator',
        'Every permission everywhere',
        perChannel: false,
      ),
    ],
  ),
];
