import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/settings/server/channels_page.dart';
import 'package:opencord/features/settings/server/invites_bans_pages.dart';
import 'package:opencord/features/settings/server/members_page.dart';
import 'package:opencord/features/settings/server/overview_page.dart';
import 'package:opencord/features/settings/server/roles_page.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_icons.dart';

/// The server settings pages the current user may open (§8.2): each only
/// with the permission it needs. [channel] is the one Channels starts on.
List<SettingsPage> serverSettingsPages(
  ServerData data,
  String serverKey, {
  int? channel,
}) => [
  if (data.can(Permissions.manageServer))
    SettingsPage(
      id: 'overview',
      label: 'Overview',
      icon: OcIcons.info,
      group: 'Server',
      builder: (_) => ServerOverviewPage(serverKey: serverKey),
    ),
  if (data.can(Permissions.manageChannels))
    SettingsPage(
      id: 'channels',
      label: 'Channels',
      icon: OcIcons.tag,
      group: 'Server',
      builder: (_) =>
          ServerChannelsPage(serverKey: serverKey, initialChannel: channel),
    ),
  if (data.can(Permissions.manageRoles))
    SettingsPage(
      id: 'roles',
      label: 'Roles',
      icon: OcIcons.shieldPerson,
      group: 'Server',
      builder: (_) => ServerRolesPage(serverKey: serverKey),
    ),
  if (data.can(Permissions.kickMembers) ||
      data.can(Permissions.banMembers) ||
      data.can(Permissions.manageRoles))
    SettingsPage(
      id: 'members',
      label: 'Members',
      icon: OcIcons.group,
      group: 'People',
      builder: (_) => ServerMembersPage(serverKey: serverKey),
    ),
  if (data.can(Permissions.createInvite))
    SettingsPage(
      id: 'invites',
      label: 'Invites',
      icon: OcIcons.personAdd,
      group: 'People',
      builder: (_) => ServerInvitesPage(serverKey: serverKey),
    ),
  if (data.can(Permissions.banMembers))
    SettingsPage(
      id: 'bans',
      label: 'Bans',
      icon: OcIcons.block,
      group: 'People',
      builder: (_) => ServerBansPage(serverKey: serverKey),
    ),
];

/// Whether the current user may open any server settings page.
bool canOpenServerSettings(ServerData? data) =>
    data != null && serverSettingsPages(data, '').isNotEmpty;

Future<void> showServerSettings(
  BuildContext context,
  WidgetRef ref, {
  required String serverKey,
  String? page,
  int? channel,
}) async {
  final data = ref.read(serverProvider(serverKey)).data;
  if (data == null) return;
  final pages = serverSettingsPages(data, serverKey, channel: channel);
  if (pages.isEmpty) return;
  await showSettingsDialog(
    context,
    title: data.info.name,
    pages: pages,
    initialPage: page,
  );
}
