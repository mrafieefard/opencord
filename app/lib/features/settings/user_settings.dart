import 'package:flutter/material.dart';

import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/settings/user/app_pages.dart';
import 'package:opencord/features/settings/user/appearance_page.dart';
import 'package:opencord/features/settings/user/profile_pages.dart';
import 'package:opencord/ui/theme/oc_icons.dart';

/// The user settings pages (§8.1). Video, screen share, sounds and voice
/// diagnostics join when the rest of §17 arrives.
List<SettingsPage> userSettingsPages(TargetPlatform platform) {
  final desktop =
      platform == TargetPlatform.linux ||
      platform == TargetPlatform.windows ||
      platform == TargetPlatform.macOS;
  return [
    SettingsPage(
      id: 'profile',
      label: 'My profile',
      icon: OcIcons.person,
      group: 'You',
      builder: (_) => const ProfilePage(),
    ),
    SettingsPage(
      id: 'identity',
      label: 'Identity & keys',
      icon: OcIcons.key,
      group: 'You',
      builder: (_) => const IdentityPage(),
    ),
    SettingsPage(
      id: 'appearance',
      label: 'Appearance',
      icon: OcIcons.palette,
      group: 'App',
      builder: (_) => const AppearancePage(),
    ),
    if (desktop)
      SettingsPage(
        id: 'behavior',
        label: 'Windows & behavior',
        icon: OcIcons.desktopWindows,
        group: 'App',
        builder: (_) => const BehaviorPage(),
      ),
    SettingsPage(
      id: 'voice',
      label: 'Voice & audio',
      icon: OcIcons.mic,
      group: 'App',
      builder: (_) => const VoicePage(),
    ),
    SettingsPage(
      id: 'notifications',
      label: 'Notifications',
      icon: OcIcons.notifications,
      group: 'App',
      builder: (_) => const NotificationsPage(),
    ),
    SettingsPage(
      id: 'keybinds',
      label: 'Keybinds',
      icon: OcIcons.keyboard,
      group: 'App',
      builder: (_) => const KeybindsPage(),
    ),
    SettingsPage(
      id: 'trusted',
      label: 'Trusted servers',
      icon: OcIcons.verifiedUser,
      group: 'Security',
      builder: (_) => const TrustedServersPage(),
    ),
    SettingsPage(
      id: 'about',
      label: 'About',
      icon: OcIcons.info,
      group: 'Opencord',
      builder: (_) => const AboutPage(),
    ),
  ];
}

Future<void> showUserSettings(BuildContext context, {String? page}) =>
    showSettingsDialog(
      context,
      title: 'Settings',
      pages: userSettingsPages(Theme.of(context).platform),
      initialPage: page,
    );
