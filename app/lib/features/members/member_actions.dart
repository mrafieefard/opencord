import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/composer_state.dart';
import 'package:opencord/features/members/member_profile.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/voice/user_audio_controls.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Whether Mention has a composer to type into: a text channel of the
/// server is open (voice channels have none).
final canMentionProvider = Provider.family<bool, String>((ref, serverKey) {
  if (ref.watch(currentServerProvider) != serverKey) return false;
  final open = ref.watch(currentChannelProvider);
  if (open == null) return false;
  final kind = ref.watch(
    serverProvider(serverKey).select((s) => s.data?.channels[open]?.kind),
  );
  return kind?.isTextLike ?? false;
});

/// What can be done to a member and doing it (§4.7): the member panel's
/// menu, the profile dialog's buttons.
class MemberActions {
  const MemberActions({
    required this.context,
    required this.ref,
    required this.serverKey,
  });

  final BuildContext context;
  final WidgetRef ref;
  final String serverKey;

  ServerData? get _data => ref.read(serverProvider(serverKey)).data;

  bool _isSelf(Member member) => member.id == _data?.self.id;

  /// Whether the current user may act on [member] with [permission] (in
  /// [channelId], when given): they hold it, and they rank above them —
  /// voice moderation may also be used on yourself, the rest may not.
  bool _may(Member member, Permissions permission, {int? channelId}) {
    final data = _data;
    final self = data?.selfMember;
    if (data == null || self == null) return false;
    final held = channelId == null
        ? data.serverPermissions
        : data.permissionsIn(channelId);
    if (!held.has(permission)) return false;
    if (_isSelf(member)) return channelId != null;
    return data
        .contextFor(self)
        .outranksMember(
          targetIsOwner: data.info.ownerId == member.id,
          targetHighest: data.contextFor(member).highestPosition,
        );
  }

  bool canKick(Member member) => _may(member, Permissions.kickMembers);

  bool canBan(Member member) => _may(member, Permissions.banMembers);

  /// Roles the current user can hand out or take away, highest first.
  List<Role> manageableRoles() {
    final data = _data;
    final self = data?.selfMember;
    if (data == null || self == null || !data.can(Permissions.manageRoles)) {
      return const [];
    }
    final context = data.contextFor(self);
    return [
      for (final role in data.rolesDescending)
        if (role.id != data.info.everyoneRoleId &&
            context.outranksRole(role.position))
          role,
    ];
  }

  /// Giving a role also needs every permission it carries (the server's
  /// rule, protocol §AddMemberRole).
  bool mayGrant(Role role) {
    final data = _data;
    final self = data?.selfMember;
    if (data == null || self == null) return false;
    return canGrant(data.contextFor(self).base, role.permissions);
  }

  void profile(Member member) =>
      showMemberProfile(context, serverKey: serverKey, userId: member.id);

  /// Puts "@Name " into the open channel's composer and focuses it.
  void mention(Member member) {
    final channel = ref.read(currentChannelProvider);
    if (channel == null || ref.read(currentServerProvider) != serverKey) return;
    final composer = composerProvider((server: serverKey, channel: channel));
    final draft = ref.read(composer).draft;
    final gap = draft.isEmpty || draft.endsWith(' ') ? '' : ' ';
    ref.read(composer.notifier).setDraft('$draft$gap@${member.displayName} ');
    ref.read(chatControllerProvider).focusComposer();
  }

  void copyFingerprint(Member member) {
    Clipboard.setData(ClipboardData(text: member.user.fingerprint));
    showOcToast(context, 'Fingerprint copied');
  }

  Future<void> _guard(Future<void> Function() action) async {
    try {
      await action();
    } on RepoException catch (error) {
      if (context.mounted) showOcToast(context, error.message);
    }
  }

  Future<void> toggleRole(Member member, Role role) => _guard(() async {
    final repository = ref.read(repositoryProvider);
    if (member.roleIds.contains(role.id)) {
      await repository.removeMemberRole(serverKey, member.id, role.id);
    } else {
      await repository.addMemberRole(serverKey, member.id, role.id);
    }
  });

  /// The voice channel [member] is in on this server, and how.
  (int, VoiceParticipant)? voiceOf(Member member) {
    for (final MapEntry(key: channelId, value: participants)
        in ref.read(voiceProvider(serverKey)).entries) {
      for (final participant in participants) {
        if (participant.userId == member.id) return (channelId, participant);
      }
    }
    return null;
  }

  /// Voice channels [member] can be moved to from [from]: Move members is
  /// needed in both (the server checks they may join).
  List<Channel> moveTargets(Member member, int from) {
    final data = _data;
    if (data == null ||
        !_may(member, Permissions.moveMembers, channelId: from)) {
      return const [];
    }
    return [
      for (final channel in data.channels.values)
        if (channel.kind == ChannelKind.voice &&
            channel.id != from &&
            data.permissionsIn(channel.id).has(Permissions.moveMembers))
          channel,
    ]..sort((a, b) => a.position.compareTo(b.position));
  }

  Future<void> serverMute(Member member, bool muted) => _guard(
    () => ref
        .read(repositoryProvider)
        .serverMuteMember(serverKey, member.id, muted),
  );

  Future<void> serverDeafen(Member member, bool deafened) => _guard(
    () => ref
        .read(repositoryProvider)
        .serverDeafenMember(serverKey, member.id, deafened),
  );

  Future<void> moveTo(Member member, int channelId) => _guard(
    () => ref
        .read(repositoryProvider)
        .moveMember(serverKey, member.id, channelId),
  );

  Future<void> disconnect(Member member) => _guard(
    () => ref.read(repositoryProvider).disconnectMember(serverKey, member.id),
  );

  /// The voice part of the menu (desktop UI plan §17.3) for someone in
  /// voice: this device's volume, Mute and Mute soundboard for them, then
  /// what moderators may do.
  List<OcMenuEntry> _voiceEntries(Member member) {
    final (channelId, participant) = voiceOf(member) ?? (null, null);
    if (channelId == null || participant == null) return const [];
    final mute = _may(member, Permissions.muteMembers, channelId: channelId);
    final deafen = _may(
      member,
      Permissions.deafenMembers,
      channelId: channelId,
    );
    final move = _may(member, Permissions.moveMembers, channelId: channelId);
    final targets = moveTargets(member, channelId);
    return [
      if (!_isSelf(member)) ...[
        const OcMenuDivider(),
        OcMenuCustom(
          (_) => UserAudioControls(serverKey: serverKey, userId: member.id),
        ),
      ],
      if (mute || deafen || move) const OcMenuDivider(),
      if (mute)
        OcMenuItem(
          label: 'Server mute',
          icon: OcIcons.micOff,
          checked: participant.serverMuted,
          onSelected: () => serverMute(member, !participant.serverMuted),
        ),
      if (deafen)
        OcMenuItem(
          label: 'Server deafen',
          icon: OcIcons.headsetOff,
          checked: participant.serverDeafened,
          onSelected: () => serverDeafen(member, !participant.serverDeafened),
        ),
      if (targets.isNotEmpty)
        OcMenuItem(
          label: 'Move to',
          icon: OcIcons.moveDown,
          submenu: [
            for (final channel in targets)
              OcMenuItem(
                label: channel.name,
                icon: OcIcons.volumeUp,
                onSelected: () => moveTo(member, channel.id),
              ),
          ],
        ),
      if (move)
        OcMenuItem(
          label: 'Disconnect',
          icon: OcIcons.callEnd,
          onSelected: () => disconnect(member),
        ),
    ];
  }

  /// Kick and ban act on someone else, so they ask first (§16).
  Future<void> kick(Member member) async {
    final reason = await _confirm(
      title: 'Kick ${member.displayName}?',
      body: 'They leave the server now and can come back with an invite.',
      action: 'Kick',
    );
    if (reason == null) return;
    await _guard(
      () => ref
          .read(repositoryProvider)
          .kickMember(
            serverKey,
            member.id,
            reason: reason.isEmpty ? null : reason,
          ),
    );
  }

  Future<void> ban(Member member) async {
    final reason = await _confirm(
      title: 'Ban ${member.displayName}?',
      body:
          'They leave the server now and cannot come back until someone '
          'lifts the ban in server settings.',
      action: 'Ban',
    );
    if (reason == null) return;
    await _guard(
      () => ref
          .read(repositoryProvider)
          .banMember(
            serverKey,
            member.id,
            reason: reason.isEmpty ? null : reason,
          ),
    );
  }

  /// The reason typed (maybe empty), or null when cancelled.
  Future<String?> _confirm({
    required String title,
    required String body,
    required String action,
  }) => showOcDialog<String>(
    context: context,
    builder: (context) =>
        _ReasonDialog(title: title, body: body, action: action),
  );

  /// The member menu (§4.7): Profile · Mention · Copy identity fingerprint
  /// · (in voice) volume, Mute, Mute soundboard, server mute and deafen,
  /// Move to ▸, Disconnect · Roles ▸ · Kick · Ban, each only when allowed.
  List<OcMenuEntry> menu(Member member) {
    final roles = manageableRoles();
    final canMention = ref.read(canMentionProvider(serverKey));
    return [
      OcMenuItem(
        label: 'Profile',
        icon: OcIcons.person,
        onSelected: () => profile(member),
      ),
      if (canMention)
        OcMenuItem(
          label: 'Mention',
          icon: OcIcons.alternateEmail,
          onSelected: () => mention(member),
        ),
      if (member.user.fingerprint.isNotEmpty)
        OcMenuItem(
          label: 'Copy identity fingerprint',
          icon: OcIcons.fingerprint,
          onSelected: () => copyFingerprint(member),
        ),
      ..._voiceEntries(member),
      if (roles.isNotEmpty) ...[
        if (voiceOf(member) != null) const OcMenuDivider(),
        OcMenuItem(
          label: 'Roles',
          icon: OcIcons.shieldPerson,
          submenu: [
            for (final role in roles)
              OcMenuItem(
                label: role.name,
                checked: member.roleIds.contains(role.id),
                // Removing needs only the rank; giving needs the role's
                // permissions too.
                onSelected: member.roleIds.contains(role.id) || mayGrant(role)
                    ? () => toggleRole(member, role)
                    : null,
              ),
          ],
        ),
      ],
      if (canKick(member) || canBan(member)) const OcMenuDivider(),
      if (canKick(member))
        OcMenuItem(
          label: 'Kick',
          icon: OcIcons.personRemove,
          onSelected: () => kick(member),
        ),
      if (canBan(member))
        OcMenuItem(
          label: 'Ban',
          icon: OcIcons.block,
          onSelected: () => ban(member),
        ),
    ];
  }

  Future<void> showMenu(Member member, Offset position) =>
      showOcMenu(context: context, position: position, entries: menu(member));
}

class _ReasonDialog extends StatefulWidget {
  const _ReasonDialog({
    required this.title,
    required this.body,
    required this.action,
  });

  final String title;
  final String body;
  final String action;

  @override
  State<_ReasonDialog> createState() => _ReasonDialogState();
}

class _ReasonDialogState extends State<_ReasonDialog> {
  final _reason = TextEditingController();

  @override
  void dispose() {
    _reason.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => OcDialog(
    title: widget.title,
    actions: [
      OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
      OcButton.primary(
        label: widget.action,
        onPressed: () => Navigator.pop(context, _reason.text.trim()),
      ),
    ],
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(widget.body),
        const SizedBox(height: OcSpace.s16),
        OcTextField(
          controller: _reason,
          autofocus: true,
          hint: 'Reason (optional)',
          semanticLabel: 'Reason',
          maxLength: 512,
          onSubmitted: (_) => Navigator.pop(context, _reason.text.trim()),
        ),
      ],
    ),
  );
}
