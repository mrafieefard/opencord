import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/plugins/plugin_page.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

import '../../support/pump.dart';

const _devKey = 'opencord.example:7710';
final _testTime = DateTime(2026, 11, 8, 12, 0);

Plugin _samplePlugin({
  String id = 'ticket-bot',
  String name = 'Ticket Bot',
  String version = '1.2.0',
  String author = 'Support Corp',
  String description = 'Manages support tickets inside Opencord.',
  String github = 'supportcorp/ticket-bot',
  String license = 'MIT',
  String keyFingerprint = 'SHA256:abc123def456',
  PluginState state = PluginState.running,
  String statusReason = '',
  int databaseBytes = 1024 * 1024 * 2, // 2 MB
  int filesBytes = 1024 * 1024 * 5, // 5 MB
}) => Plugin(
  id: id,
  name: name,
  version: version,
  author: author,
  description: description,
  github: github,
  license: license,
  keyFingerprint: keyFingerprint,
  state: state,
  statusReason: statusReason,
  userId: 9002,
  roleId: 92,
  installedAt: _testTime.subtract(const Duration(days: 10)),
  updatedAt: _testTime.subtract(const Duration(days: 2)),
  databaseBytes: databaseBytes,
  filesBytes: filesBytes,
  permissions: const [
    PluginPermissionGrant(
      name: 'send_messages',
      granted: true,
      grantable: true,
    ),
    PluginPermissionGrant(
      name: 'read_messages',
      granted: false,
      grantable: true,
    ),
  ],
  domains: const ['api.tickets.example.com'],
  configFields: const [
    PluginConfigField(
      key: 'api_url',
      label: 'API URL',
      kind: PluginConfigKind.text,
      required: true,
    ),
    PluginConfigField(
      key: 'api_key',
      label: 'API Key',
      kind: PluginConfigKind.secret,
      required: true,
    ),
  ],
  config: const [
    PluginConfigValue(
      key: 'api_url',
      value: 'https://api.tickets.example.com/v1',
      hasValue: true,
    ),
    PluginConfigValue(
      key: 'api_key',
      value: 'existing-secret-key-12345',
      hasValue: true,
    ),
  ],
);

class _FakePluginPageRepository implements OpencordRepository {
  _FakePluginPageRepository({
    required this.plugin,
    List<PluginDevToken>? initialTokens,
  }) : tokens = initialTokens ?? [];

  Plugin plugin;
  final List<PluginDevToken> tokens;
  final StreamController<RepoEvent> _events = StreamController.broadcast();

  bool restarted = false;
  bool? uninstalledDeleteData;
  List<PluginConfigValue>? lastSavedConfig;
  List<String>? lastSavedPermissions;
  int? revokedTokenId;
  int? createdTokenDays;

  @override
  Stream<RepoEvent> get events => _events.stream;

  @override
  Future<List<Plugin>> fetchPlugins(String serverKey) async => [plugin];

  @override
  Future<Plugin> restartPlugin(String serverKey, String pluginId) async {
    restarted = true;
    plugin = plugin.copyWith(state: PluginState.running, statusReason: '');
    _events.add(PluginUpdated(serverKey, plugin));
    return plugin;
  }

  @override
  Future<void> uninstallPlugin(
    String serverKey,
    String pluginId, {
    required bool deleteData,
  }) async {
    uninstalledDeleteData = deleteData;
    _events.add(PluginRemoved(serverKey, pluginId));
  }

  @override
  Future<Plugin> setPluginPermissions(
    String serverKey,
    String pluginId,
    List<String> permissions,
  ) async {
    lastSavedPermissions = permissions;
    final updatedGrants = plugin.permissions.map((p) {
      return PluginPermissionGrant(
        name: p.name,
        granted: permissions.contains(p.name),
        grantable: p.grantable,
      );
    }).toList();
    plugin = plugin.copyWith(permissions: updatedGrants);
    _events.add(PluginUpdated(serverKey, plugin));
    return plugin;
  }

  @override
  Future<Plugin> setPluginConfig(
    String serverKey,
    String pluginId,
    List<PluginConfigValue> config,
  ) async {
    lastSavedConfig = config;
    plugin = plugin.copyWith(config: config);
    _events.add(PluginUpdated(serverKey, plugin));
    return plugin;
  }

  @override
  Future<List<PluginDevToken>> fetchPluginDevTokens(String serverKey) async =>
      tokens;

  @override
  Future<CreatedPluginDevToken> createPluginDevToken(
    String serverKey,
    String pluginId, {
    int days = 7,
  }) async {
    createdTokenDays = days;
    final token = PluginDevToken(
      id: tokens.length + 1,
      pluginId: pluginId,
      createdBy: 1000,
      createdAt: DateTime.now(),
      expiresAt: DateTime.now().add(Duration(days: days)),
      lastUsedAt: null,
    );
    tokens.add(token);
    return CreatedPluginDevToken(
      token: token,
      secret: 'ocdev_secret_xyz123',
      serverFingerprint: 'SHA256:srvfp7710',
    );
  }

  @override
  Future<void> revokePluginDevToken(String serverKey, int tokenId) async {
    revokedTokenId = tokenId;
    tokens.removeWhere((t) => t.id == tokenId);
  }

  @override
  Future<List<PluginLogLine>> fetchPluginLog(
    String serverKey,
    String pluginId, {
    int? before,
    int limit = 100,
  }) async {
    return [
      PluginLogLine(
        seq: 1,
        at: _testTime.subtract(const Duration(seconds: 10)),
        level: PluginLogLevel.info,
        text: 'Plugin started successfully',
      ),
      PluginLogLine(
        seq: 2,
        at: _testTime.subtract(const Duration(seconds: 5)),
        level: PluginLogLevel.action,
        text: 'Created channel #ticket-101',
      ),
      PluginLogLine(
        seq: 3,
        at: _testTime.subtract(const Duration(seconds: 1)),
        level: PluginLogLevel.error,
        text: 'HTTP request timed out connecting to api.tickets.example.com',
      ),
    ];
  }

  @override
  Future<void> watchPluginLog(
    String serverKey,
    String pluginId, {
    required bool watch,
  }) async {}

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _FakeServerNotifier extends ServerNotifier {
  _FakeServerNotifier(super.serverKey, this.initialData);
  final ServerData initialData;

  @override
  ServerState build() => ServerState(
    key: serverKey,
    connection: const ConnectionStatus.connected(),
    data: initialData,
  );
}

class _EventRouter extends ConsumerStatefulWidget {
  const _EventRouter({required this.child});
  final Widget child;

  @override
  ConsumerState<_EventRouter> createState() => _EventRouterState();
}

class _EventRouterState extends ConsumerState<_EventRouter> {
  StreamSubscription? _sub;

  @override
  void initState() {
    super.initState();
    final repo = ref.read(repositoryProvider);
    _sub = repo.events.listen((event) {
      final key = event.serverKey;
      ref.read(pluginsProvider(key).notifier).apply(event);
    });
  }

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}

Widget _wrap(
  Widget child, {
  required _FakePluginPageRepository repository,
  OcColors colors = OcColors.dark,
}) {
  final data = const ServerData(
    self: User(id: 1000, displayName: 'Admin'),
    info: ServerInfo(serverIdHex: '01', name: 'Test Server', everyoneRoleId: 1),
    channels: {},
    roles: {},
    members: {},
    serverPermissions: Permissions.managePlugins,
    channelPermissions: {},
  );

  return ProviderScope(
    overrides: [
      repositoryProvider.overrideWithValue(repository),
      serverProvider(
        _devKey,
      ).overrideWith(() => _FakeServerNotifier(_devKey, data)),
    ],
    child: _EventRouter(
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: buildTheme(colors),
        home: Scaffold(backgroundColor: colors.chat, body: child),
      ),
    ),
  );
}

void main() {
  testWidgets('plugin_page_overview_section', (tester) async {
    final plugin = _samplePlugin();
    final repo = _FakePluginPageRepository(plugin: plugin);

    await tester.pumpWidget(
      _wrap(
        PluginPage(serverKey: _devKey, pluginId: plugin.id),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    // Title and metadata
    expect(find.text('Ticket Bot'), findsWidgets);
    expect(find.text('1.2.0'), findsWidgets);
    expect(find.text('Support Corp'), findsWidgets);
    expect(
      find.text('Manages support tickets inside Opencord.'),
      findsOneWidget,
    );

    // Links and details
    expect(find.text('MIT'), findsOneWidget);
    expect(find.text('SHA256:abc123def456'), findsOneWidget);

    // Storage quotas
    expect(find.text('2.0 MB'), findsOneWidget);
    expect(find.text('5.0 MB'), findsOneWidget);

    // Action buttons in overview
    expect(find.text('Restart'), findsOneWidget);
    expect(find.text('Update'), findsOneWidget);
    expect(find.text('Uninstall'), findsOneWidget);

    // Tapping Restart calls repository
    await tester.ensureVisible(find.text('Restart'));
    await tester.tap(find.text('Restart'));
    await tester.pump(const Duration(milliseconds: 2000));
    await tester.pumpAndSettle();
    expect(repo.restarted, true);
  });

  testWidgets('crashed_plugin_shows_start_again', (tester) async {
    final plugin = _samplePlugin(
      state: PluginState.crashed,
      statusReason: 'Out of memory: 32MB limit exceeded',
    );
    final repo = _FakePluginPageRepository(plugin: plugin);

    await tester.pumpWidget(
      _wrap(
        PluginPage(serverKey: _devKey, pluginId: plugin.id),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('Out of memory: 32MB limit exceeded'), findsOneWidget);
    expect(find.text('Start again'), findsOneWidget);

    await tester.tap(find.text('Start again'));
    await tester.pump(const Duration(milliseconds: 2000));
    await tester.pumpAndSettle();
    expect(repo.restarted, true);
  });

  testWidgets('plugin_page_permissions_section_and_save', (tester) async {
    final plugin = _samplePlugin();
    final repo = _FakePluginPageRepository(plugin: plugin);

    await tester.pumpWidget(
      _wrap(
        PluginPage(serverKey: _devKey, pluginId: plugin.id),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    // Switch to Permissions tab
    await tester.tap(find.text('Permissions'));
    await tester.pumpAndSettle();

    expect(find.text('send_messages'), findsOneWidget);
    expect(find.text('read_messages'), findsOneWidget);
    expect(find.text('api.tickets.example.com'), findsOneWidget);

    // Toggle read_messages on
    final switches = find.byType(OcSwitch);
    await tester.tap(switches.at(1));
    await tester.pumpAndSettle();

    // Save bar shows
    final saveButton = find.widgetWithText(OcButton, 'Save changes');
    expect(saveButton, findsOneWidget);

    await tester.tap(saveButton);
    await tester.pumpAndSettle();

    expect(repo.lastSavedPermissions, contains('read_messages'));
    expect(repo.lastSavedPermissions, contains('send_messages'));
  });

  testWidgets('a_kept_secret_is_sent_empty', (tester) async {
    final plugin = _samplePlugin();
    final repo = _FakePluginPageRepository(plugin: plugin);

    await tester.pumpWidget(
      _wrap(
        PluginPage(serverKey: _devKey, pluginId: plugin.id),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    // Switch to Settings tab
    await tester.tap(find.text('Settings'));
    await tester.pumpAndSettle();

    expect(find.text('API URL *'), findsOneWidget);
    expect(find.text('API Key *'), findsOneWidget);

    // Secret field has hint "Saved. Leave empty to keep it." and its controller text is empty
    expect(find.text('Saved. Leave empty to keep it.'), findsOneWidget);

    // Change API URL
    final urlField = find.widgetWithText(
      TextField,
      'https://api.tickets.example.com/v1',
    );
    expect(urlField, findsOneWidget);
    await tester.enterText(urlField, 'https://api.tickets.example.com/v2');
    await tester.pumpAndSettle();

    // Save bar appears
    final saveButton = find.widgetWithText(OcButton, 'Save changes');
    expect(saveButton, findsOneWidget);
    await tester.tap(saveButton);
    await tester.pumpAndSettle();

    expect(repo.lastSavedConfig, isNotNull);
    final secretVal = repo.lastSavedConfig!.firstWhere(
      (c) => c.key == 'api_key',
    );
    // The secret must be empty string! Never sending back the secret or masked value.
    expect(secretVal.value, '');
    expect(secretVal.hasValue, false);

    final urlVal = repo.lastSavedConfig!.firstWhere((c) => c.key == 'api_url');
    expect(urlVal.value, 'https://api.tickets.example.com/v2');
    expect(urlVal.hasValue, true);
  });

  testWidgets('uninstall_asks_about_data', (tester) async {
    final plugin = _samplePlugin();
    final repo = _FakePluginPageRepository(plugin: plugin);

    await tester.pumpWidget(
      _wrap(
        PluginPage(serverKey: _devKey, pluginId: plugin.id),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    await tester.ensureVisible(find.text('Uninstall'));
    await tester.tap(find.text('Uninstall'));
    await tester.pumpAndSettle();

    // Dialog title
    expect(find.text('Uninstall Ticket Bot?'), findsOneWidget);
    expect(find.text('Also delete its data and files'), findsOneWidget);

    // Check the box
    await tester.tap(find.text('Also delete its data and files'));
    await tester.pumpAndSettle();

    // Tap confirm Uninstall in dialog
    final confirmButton = find.descendant(
      of: find.byType(OcDialog),
      matching: find.widgetWithText(OcButton, 'Uninstall'),
    );
    await tester.tap(confirmButton);
    await tester.pump(const Duration(milliseconds: 2000));
    await tester.pumpAndSettle();

    expect(repo.uninstalledDeleteData, true);
  });

  testWidgets('a_new_token_is_shown_once', (tester) async {
    final plugin = _samplePlugin();
    final repo = _FakePluginPageRepository(
      plugin: plugin,
      initialTokens: [
        PluginDevToken(
          id: 10,
          pluginId: plugin.id,
          createdBy: 1000,
          createdAt: _testTime.subtract(const Duration(days: 1)),
          expiresAt: _testTime.add(const Duration(days: 6)),
          lastUsedAt: _testTime,
        ),
      ],
    );

    await tester.pumpWidget(
      _wrap(
        PluginPage(serverKey: _devKey, pluginId: plugin.id),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    // Switch to Dev tokens tab
    await tester.tap(find.text('Dev tokens'));
    await tester.pumpAndSettle();

    expect(find.text('Create a dev token'), findsOneWidget);
    expect(find.text('Revoke'), findsOneWidget);

    // Open Create a dev token dialog
    await tester.tap(find.text('Create a dev token'));
    await tester.pumpAndSettle();

    expect(find.text('Create dev token'), findsOneWidget);
    expect(find.text('Days (1–30)'), findsOneWidget);

    // Submit token creation
    await tester.tap(find.widgetWithText(OcButton, 'Create'));
    await tester.pumpAndSettle();

    // Token secret and ready command are shown
    expect(find.text('ocdev_secret_xyz123'), findsOneWidget);
    expect(
      find.text(
        'opencord-plugin dev --server opencord.example:7710 --fingerprint SHA256:srvfp7710 --token ocdev_secret_xyz123',
      ),
      findsOneWidget,
    );
    expect(find.text('Copy'), findsWidgets);
    expect(find.text('Done'), findsOneWidget);

    await tester.tap(find.text('Done'));
    await tester.pumpAndSettle();

    // Revoke existing token
    await tester.tap(find.text('Revoke').first);
    await tester.pump(const Duration(milliseconds: 2000));
    await tester.pumpAndSettle();
    expect(repo.revokedTokenId, 10);
  });

  testWidgets('log_view_keeps_up_with_fast_plugin_and_filters', (tester) async {
    final plugin = _samplePlugin();
    final repo = _FakePluginPageRepository(plugin: plugin);

    await tester.pumpWidget(
      _wrap(
        PluginPage(serverKey: _devKey, pluginId: plugin.id),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    // Switch to Log tab
    await tester.tap(find.text('Log'));
    await tester.pumpAndSettle();

    expect(find.text('Plugin started successfully'), findsOneWidget);
    expect(find.text('Created channel #ticket-101'), findsOneWidget);
    expect(
      find.text('HTTP request timed out connecting to api.tickets.example.com'),
      findsOneWidget,
    );

    // Filter by Errors
    await tester.tap(find.text('Errors'));
    await tester.pumpAndSettle();

    expect(find.text('Plugin started successfully'), findsNothing);
    expect(find.text('Created channel #ticket-101'), findsNothing);
    expect(
      find.text('HTTP request timed out connecting to api.tickets.example.com'),
      findsOneWidget,
    );

    // Filter by Actions
    await tester.tap(find.text('Actions'));
    await tester.pumpAndSettle();

    expect(find.text('Created channel #ticket-101'), findsOneWidget);
    expect(
      find.text('HTTP request timed out connecting to api.tickets.example.com'),
      findsNothing,
    );
  });

  for (final (name, colors) in themes) {
    testWidgets('plugin page overview in $name', (tester) async {
      final plugin = _samplePlugin();
      final repo = _FakePluginPageRepository(plugin: plugin);

      await tester.pumpWidget(
        _wrap(
          PluginPage(serverKey: _devKey, pluginId: plugin.id),
          repository: repo,
          colors: colors,
        ),
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(PluginPage),
        matchesGoldenFile('goldens/plugin_page_overview_$name.png'),
      );
    });

    testWidgets('plugin page settings in $name', (tester) async {
      final plugin = _samplePlugin();
      final repo = _FakePluginPageRepository(plugin: plugin);

      await tester.pumpWidget(
        _wrap(
          PluginPage(
            serverKey: _devKey,
            pluginId: plugin.id,
            initialTab: PluginPageTab.settings,
          ),
          repository: repo,
          colors: colors,
        ),
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(PluginPage),
        matchesGoldenFile('goldens/plugin_page_settings_$name.png'),
      );
    });
  }
}
