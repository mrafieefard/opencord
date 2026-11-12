import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/plugin_views.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/plugins/plugin_area.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../support/app.dart';

void main() {
  const serverKey = 'opencord.example:7710';

  for (final themeName in ['dark', 'light']) {
    final themePref = themeName == 'dark'
        ? ThemePreference.dark
        : ThemePreference.light;

    for (final width in [800.0, 360.0]) {
      final wLabel = width.toInt();

      testWidgets('plugin area running (${wLabel}w, $themeName)', (
        tester,
      ) async {
        final app = await MockApp.pump(
          tester,
          size: Size(width, 600),
          settings: (s) => s.copyWith(theme: themePref),
        );
        final server = app.repository.mockServer(serverKey);
        final ticketsId = server.channels.values
            .firstWhere((c) => c.name == 'tickets')
            .id;

        app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
        await tester.pumpAndSettle();

        await expectLater(
          find.byType(PluginArea),
          matchesGoldenFile(
            'goldens/plugin_area_running_${wLabel}_$themeName.png',
          ),
        );
        await app.dispose(tester);
      });

      testWidgets('plugin area error with details (${wLabel}w, $themeName)', (
        tester,
      ) async {
        final app = await MockApp.pump(
          tester,
          size: Size(width, 600),
          settings: (s) => s.copyWith(theme: themePref),
        );
        final server = app.repository.mockServer(serverKey);
        final ticketsId = server.channels.values
            .firstWhere((c) => c.name == 'tickets')
            .id;

        app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
        await tester.pumpAndSettle();

        final controller = app.read(
          pluginViewProvider(ChannelPlace(serverKey, ticketsId)),
        );
        controller.reportError('Failed', 'Crash: Division by zero at line 42');
        await tester.pumpAndSettle();

        await expectLater(
          find.byType(PluginArea),
          matchesGoldenFile(
            'goldens/plugin_area_error_details_${wLabel}_$themeName.png',
          ),
        );
        await app.dispose(tester);
      });

      testWidgets(
        'plugin area error without details (${wLabel}w, $themeName)',
        (tester) async {
          final app = await MockApp.pump(
            tester,
            size: Size(width, 600),
            settings: (s) => s.copyWith(theme: themePref),
          );
          final server = app.repository.mockServer(serverKey);
          final ticketsId = server.channels.values
              .firstWhere((c) => c.name == 'tickets')
              .id;

          server.info = ServerInfo(
            serverIdHex: server.info.serverIdHex,
            name: server.info.name,
            ownerId: 999999,
            everyoneRoleId: server.info.everyoneRoleId,
          );
          server.roles[server.info.everyoneRoleId] = Role(
            id: server.info.everyoneRoleId,
            name: '@everyone',
            permissions: Permissions.viewChannel,
            position: 0,
          );
          server.members[server.selfId] = server.members[server.selfId]!
              .copyWith(roleIds: []);
          app.repository.emit(Ready(serverKey, server.snapshot()));
          await tester.pumpAndSettle();

          app
              .read(navigationProvider.notifier)
              .openChannel(serverKey, ticketsId);
          await tester.pumpAndSettle();

          final controller = app.read(
            pluginViewProvider(ChannelPlace(serverKey, ticketsId)),
          );
          controller.reportError(
            'Failed',
            'Crash: Division by zero at line 42',
          );
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(PluginArea),
            matchesGoldenFile(
              'goldens/plugin_area_error_no_details_${wLabel}_$themeName.png',
            ),
          );
          await app.dispose(tester);
        },
      );

      testWidgets('plugin area too old (${wLabel}w, $themeName)', (
        tester,
      ) async {
        final app = await MockApp.pump(
          tester,
          size: Size(width, 600),
          settings: (s) => s.copyWith(theme: themePref),
        );
        final server = app.repository.mockServer(serverKey);
        final ticketsId = server.channels.values
            .firstWhere((c) => c.name == 'tickets')
            .id;

        app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
        await tester.pumpAndSettle();

        final controller = app.read(
          pluginViewProvider(ChannelPlace(serverKey, ticketsId)),
        );
        controller.applyBatch(
          jsonEncode({
            'view': 'v',
            'seq': 99,
            'ops': [
              {'op': 'too_old'},
            ],
          }),
        );
        await tester.pumpAndSettle();

        await expectLater(
          find.byType(PluginArea),
          matchesGoldenFile(
            'goldens/plugin_area_too_old_${wLabel}_$themeName.png',
          ),
        );
        await app.dispose(tester);
      });

      testWidgets(
        'plugin area closed plugin_stopped (${wLabel}w, $themeName)',
        (tester) async {
          final app = await MockApp.pump(
            tester,
            size: Size(width, 600),
            settings: (s) => s.copyWith(theme: themePref),
          );
          final server = app.repository.mockServer(serverKey);
          final ticketsId = server.channels.values
              .firstWhere((c) => c.name == 'tickets')
              .id;

          app
              .read(navigationProvider.notifier)
              .openChannel(serverKey, ticketsId);
          await tester.pumpAndSettle();

          final controller = app.read(
            pluginViewProvider(ChannelPlace(serverKey, ticketsId)),
          );
          controller.applyBatch(
            jsonEncode({
              'view': 'v',
              'seq': 99,
              'ops': [
                {'op': 'closed', 'reason': 'plugin_stopped'},
              ],
            }),
          );
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(PluginArea),
            matchesGoldenFile(
              'goldens/plugin_area_closed_stopped_${wLabel}_$themeName.png',
            ),
          );
          await app.dispose(tester);
        },
      );

      testWidgets('plugin area closed lost_access (${wLabel}w, $themeName)', (
        tester,
      ) async {
        final app = await MockApp.pump(
          tester,
          size: Size(width, 600),
          settings: (s) => s.copyWith(theme: themePref),
        );
        final server = app.repository.mockServer(serverKey);
        final ticketsId = server.channels.values
            .firstWhere((c) => c.name == 'tickets')
            .id;

        app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
        await tester.pumpAndSettle();

        final controller = app.read(
          pluginViewProvider(ChannelPlace(serverKey, ticketsId)),
        );
        controller.applyBatch(
          jsonEncode({
            'view': 'v',
            'seq': 99,
            'ops': [
              {'op': 'closed', 'reason': 'lost_access'},
            ],
          }),
        );
        await tester.pumpAndSettle();

        await expectLater(
          find.byType(PluginArea),
          matchesGoldenFile(
            'goldens/plugin_area_closed_lost_access_${wLabel}_$themeName.png',
          ),
        );
        await app.dispose(tester);
      });

      testWidgets(
        'plugin area closed channel_deleted (${wLabel}w, $themeName)',
        (tester) async {
          final app = await MockApp.pump(
            tester,
            size: Size(width, 600),
            settings: (s) => s.copyWith(theme: themePref),
          );
          final server = app.repository.mockServer(serverKey);
          final ticketsId = server.channels.values
              .firstWhere((c) => c.name == 'tickets')
              .id;

          app
              .read(navigationProvider.notifier)
              .openChannel(serverKey, ticketsId);
          await tester.pumpAndSettle();

          final controller = app.read(
            pluginViewProvider(ChannelPlace(serverKey, ticketsId)),
          );
          controller.applyBatch(
            jsonEncode({
              'view': 'v',
              'seq': 99,
              'ops': [
                {'op': 'closed', 'reason': 'channel_deleted'},
              ],
            }),
          );
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(PluginArea),
            matchesGoldenFile(
              'goldens/plugin_area_closed_deleted_${wLabel}_$themeName.png',
            ),
          );
          await app.dispose(tester);
        },
      );

      testWidgets('plugin area closed unavailable (${wLabel}w, $themeName)', (
        tester,
      ) async {
        final app = await MockApp.pump(
          tester,
          size: Size(width, 600),
          settings: (s) => s.copyWith(theme: themePref),
        );
        final server = app.repository.mockServer(serverKey);
        final ticketsId = server.channels.values
            .firstWhere((c) => c.name == 'tickets')
            .id;

        app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
        await tester.pumpAndSettle();

        final controller = app.read(
          pluginViewProvider(ChannelPlace(serverKey, ticketsId)),
        );
        controller.applyBatch(
          jsonEncode({
            'view': 'v',
            'seq': 99,
            'ops': [
              {'op': 'closed', 'reason': 'unavailable'},
            ],
          }),
        );
        await tester.pumpAndSettle();

        await expectLater(
          find.byType(PluginArea),
          matchesGoldenFile(
            'goldens/plugin_area_closed_unavailable_${wLabel}_$themeName.png',
          ),
        );
        await app.dispose(tester);
      });

      testWidgets('plugin area unavailable typeGone (${wLabel}w, $themeName)', (
        tester,
      ) async {
        final app = await MockApp.pump(
          tester,
          size: Size(width, 600),
          settings: (s) => s.copyWith(theme: themePref),
        );
        final server = app.repository.mockServer(serverKey);
        final ticketsId = server.channels.values
            .firstWhere((c) => c.name == 'tickets')
            .id;
        server.channels[ticketsId] = server.channels[ticketsId]!.copyWith(
          pluginState: () => PluginChannelState.typeGone,
        );
        app.repository.emit(Ready(serverKey, server.snapshot()));
        await tester.pumpAndSettle();

        app.read(navigationProvider.notifier).openChannel(serverKey, ticketsId);
        await tester.pumpAndSettle();

        await expectLater(
          find.byType(PluginArea),
          matchesGoldenFile(
            'goldens/plugin_area_type_gone_${wLabel}_$themeName.png',
          ),
        );
        await app.dispose(tester);
      });

      testWidgets(
        'plugin area unavailable pluginStopped (${wLabel}w, $themeName)',
        (tester) async {
          final app = await MockApp.pump(
            tester,
            size: Size(width, 600),
            settings: (s) => s.copyWith(theme: themePref),
          );
          final server = app.repository.mockServer(serverKey);
          final legacyId = server.channels.values
              .firstWhere((c) => c.name == 'legacy-help')
              .id;

          app
              .read(navigationProvider.notifier)
              .openChannel(serverKey, legacyId);
          await tester.pumpAndSettle();

          await expectLater(
            find.byType(PluginArea),
            matchesGoldenFile(
              'goldens/plugin_area_plugin_stopped_${wLabel}_$themeName.png',
            ),
          );
          await app.dispose(tester);
        },
      );
    }
  }
}
