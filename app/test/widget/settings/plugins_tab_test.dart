import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/plugins/plugin_config_fields.dart';
import 'package:opencord/features/settings/server/plugins/plugins_page.dart';
import 'package:opencord/features/settings/server/plugins/review_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';

import '../../support/pump.dart';

const _devKey = 'opencord.example:7710';

final _testTime = DateTime(2026, 11, 8, 12, 0);

Plugin _samplePlugin({
  String id = 'welcome-bot',
  String name = 'Welcome Bot',
  String version = '0.1.0',
  String author = 'Amin',
  String description = 'Greets new members',
  String license = 'MIT',
  PluginState state = PluginState.running,
  PluginSignature signature = PluginSignature.signed,
  GithubCheck githubCheck = GithubCheck.verified,
  String github = 'amin/welcome-bot',
  int cpuPermille = 15,
  int memoryBytes = 32 * 1024 * 1024,
}) => Plugin(
  id: id,
  name: name,
  version: version,
  author: author,
  description: description,
  license: license,
  state: state,
  signature: signature,
  githubCheck: githubCheck,
  github: github,
  cpuPermille: cpuPermille,
  memoryBytes: memoryBytes,
  userId: 9001,
  roleId: 91,
  installedAt: _testTime,
  updatedAt: _testTime,
  permissions: const [
    PluginPermissionGrant(
      name: 'send_messages',
      granted: true,
      grantable: true,
    ),
  ],
);

PluginReview _sampleReview({
  String reviewId = 'rev-1',
  Plugin? plugin,
  Plugin? installed,
  List<String> newPermissions = const [],
  bool licenseNeedsAcceptance = false,
  DateTime? expiresAt,
  bool dev = false,
}) => PluginReview(
  reviewId: reviewId,
  plugin:
      plugin ??
      _samplePlugin(
        id: 'new-plugin',
        name: 'Ticket Master',
        version: '1.0.0',
        author: 'Support Team',
        description: 'Tickets and support for your server',
        license: 'Custom Commercial',
        signature: PluginSignature.unsigned,
        githubCheck: GithubCheck.unspecified,
      ).copyWith(
        permissions: const [
          PluginPermissionGrant(
            name: 'send_messages',
            granted: true,
            grantable: true,
          ),
          PluginPermissionGrant(
            name: 'manage_channels',
            granted: false,
            grantable: false,
          ),
        ],
        domains: const ['api.support.example.com'],
        configFields: const [
          PluginConfigField(
            key: 'channel_id',
            label: 'Alert Channel',
            kind: PluginConfigKind.text,
            required: true,
          ),
        ],
      ),
  installed: installed,
  newPermissions: newPermissions,
  licenseNeedsAcceptance: licenseNeedsAcceptance,
  expiresAt: expiresAt ?? DateTime.now().add(const Duration(minutes: 15)),
  dev: dev,
);

class _FakePluginsRepository implements OpencordRepository {
  _FakePluginsRepository({
    List<Plugin>? initialPlugins,
    List<PluginReview>? initialReviews,
  }) : plugins = initialPlugins ?? [],
       reviews = {for (final r in (initialReviews ?? [])) r.reviewId: r};

  final List<Plugin> plugins;
  final Map<String, PluginReview> reviews;
  final StreamController<RepoEvent> _events = StreamController.broadcast();

  String? lastToggledPluginId;
  bool? lastToggledRunning;
  bool confirmShouldFail = false;
  bool uploadShouldFail = false;
  bool emitRemovalBeforeConfirm = false;
  Duration confirmDelay = Duration.zero;
  String? confirmedReviewId;
  List<PluginConfigValue>? lastConfirmedConfig;

  @override
  Stream<RepoEvent> get events => _events.stream;

  @override
  Future<List<Plugin>> fetchPlugins(String serverKey) async => plugins;

  @override
  Future<List<PluginReview>> fetchPluginReviews(String serverKey) async =>
      reviews.values.toList();

  @override
  Future<Plugin> setPluginRunning(
    String serverKey,
    String pluginId,
    bool running,
  ) async {
    lastToggledPluginId = pluginId;
    lastToggledRunning = running;
    final index = plugins.indexWhere((p) => p.id == pluginId);
    if (index >= 0) {
      final updated = plugins[index].copyWith(
        state: running ? PluginState.running : PluginState.stopped,
      );
      plugins[index] = updated;
      _events.add(PluginUpdated(serverKey, updated));
      return updated;
    }
    throw const RepoException(RepoErrorKind.other, 'Plugin not found');
  }

  @override
  Future<PluginReview> uploadPluginPackage(
    String serverKey,
    Uint8List bytes,
  ) async {
    if (uploadShouldFail) {
      throw const RepoException(
        RepoErrorKind.other,
        'Corrupt or invalid zip archive',
      );
    }
    final rev = _sampleReview();
    reviews[rev.reviewId] = rev;
    _events.add(PluginReviewUpdated(serverKey, rev));
    return rev;
  }

  @override
  Future<Plugin> confirmPluginReview(
    String serverKey,
    String reviewId,
    List<String> granted,
    List<PluginConfigValue> config,
    bool acceptLicense,
  ) async {
    lastConfirmedConfig = config;
    if (emitRemovalBeforeConfirm) {
      reviews.remove(reviewId);
      _events.add(PluginReviewRemoved(serverKey, reviewId));
      if (confirmDelay > Duration.zero) {
        await Future.delayed(confirmDelay);
      }
    }
    if (confirmShouldFail) {
      throw const RepoException(
        RepoErrorKind.other,
        'Signature verification failed on server',
      );
    }
    confirmedReviewId = reviewId;
    final rev = reviews.remove(reviewId);
    if (rev != null) {
      _events.add(PluginReviewRemoved(serverKey, reviewId));
      plugins.add(rev.plugin);
      _events.add(PluginUpdated(serverKey, rev.plugin));
      return rev.plugin;
    }
    if (emitRemovalBeforeConfirm) {
      return _samplePlugin();
    }
    throw const RepoException(RepoErrorKind.other, 'Review expired');
  }

  @override
  Future<void> discardPluginReview(String serverKey, String reviewId) async {
    reviews.remove(reviewId);
    _events.add(PluginReviewRemoved(serverKey, reviewId));
  }

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
      ref.read(pluginReviewsProvider(key).notifier).apply(event);
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

/// A page with a button that opens the dialog, so a pop has a route to close
/// and a page under it that a second pop would take away.
Widget _dialogOverPage(PluginReview review) => Builder(
  builder: (context) => Scaffold(
    body: Center(
      child: ElevatedButton(
        onPressed: () => showDialog<void>(
          context: context,
          builder: (_) => ReviewDialog(serverKey: _devKey, review: review),
        ),
        child: const Text('Open Dialog'),
      ),
    ),
  ),
);

/// Lists [review] in the provider the dialog listens to and feeds it the
/// fake's events, as the app's router does. Without it a removal event never
/// reaches the dialog.
Future<void> _routeReviewEvents(
  WidgetTester tester,
  _FakePluginsRepository repository,
  PluginReview review,
) async {
  final container = ProviderScope.containerOf(
    tester.element(find.byType(ReviewDialog)),
  );
  final notifier = container.read(pluginReviewsProvider(_devKey).notifier);
  notifier.apply(PluginReviewUpdated(_devKey, review));
  final routed = repository.events.listen(notifier.apply);
  addTearDown(routed.cancel);
  await tester.pumpAndSettle();
}

Widget _wrap(
  Widget child, {
  required _FakePluginsRepository repository,
  ServerData? serverData,
  OcColors colors = OcColors.dark,
}) {
  final data =
      serverData ??
      const ServerData(
        self: User(id: 1000, displayName: 'Admin'),
        info: ServerInfo(
          serverIdHex: '01',
          name: 'Test Server',
          everyoneRoleId: 1,
        ),
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
  testWidgets('plugins_tab_shows_empty_state_when_no_plugins', (tester) async {
    final repo = _FakePluginsRepository(initialPlugins: []);
    await tester.pumpWidget(
      _wrap(const ServerPluginsPage(serverKey: _devKey), repository: repo),
    );
    await tester.pumpAndSettle();

    expect(find.text('No plugins yet.'), findsOneWidget);
    expect(
      find.text("Drop a plugin's .zip here, or choose a file"),
      findsOneWidget,
    );
    expect(find.text('Choose a file'), findsOneWidget);
  });

  testWidgets('plugins_tab_shows_plugins_list_with_metrics_and_switch', (
    tester,
  ) async {
    final running = _samplePlugin(
      id: 'welcome-bot',
      name: 'Welcome Bot',
      version: '0.1.0',
      author: 'Amin',
      state: PluginState.running,
      signature: PluginSignature.signed,
      githubCheck: GithubCheck.verified,
      cpuPermille: 15,
      memoryBytes: 32 * 1024 * 1024,
    );
    final crashed = _samplePlugin(
      id: 'dice-roller',
      name: 'Dice Roller',
      version: '0.2.1',
      author: 'Community',
      state: PluginState.crashed,
      signature: PluginSignature.unsigned,
      githubCheck: GithubCheck.unspecified,
      cpuPermille: 0,
      memoryBytes: 4 * 1024 * 1024,
    );

    final repo = _FakePluginsRepository(initialPlugins: [running, crashed]);
    await tester.pumpWidget(
      _wrap(const ServerPluginsPage(serverKey: _devKey), repository: repo),
    );
    await tester.pumpAndSettle();

    // Welcome bot row
    expect(find.text('Welcome Bot'), findsOneWidget);
    expect(find.text('0.1.0'), findsOneWidget);
    expect(find.text('Amin'), findsOneWidget);
    expect(find.text('Verified'), findsOneWidget);
    expect(find.text('Running'), findsOneWidget);
    expect(find.text('1.5%'), findsOneWidget);
    expect(find.text('32.0 MB'), findsOneWidget);

    // Dice roller row
    expect(find.text('Dice Roller'), findsOneWidget);
    expect(find.text('0.2.1'), findsOneWidget);
    expect(find.text('Community'), findsOneWidget);
    expect(find.text('Unsigned'), findsOneWidget);
    expect(find.text('Crashed'), findsOneWidget);

    // Toggling Welcome Bot's switch off
    final switches = find.byType(OcSwitch);
    expect(switches, findsNWidgets(2));

    await tester.tap(switches.first);
    await tester.pumpAndSettle();

    expect(repo.lastToggledPluginId, 'welcome-bot');
    expect(repo.lastToggledRunning, false);
  });

  testWidgets('reviews_waiting_from_dev_pushes_banner', (tester) async {
    final devReview = _sampleReview(
      reviewId: 'rev-dev',
      plugin: _samplePlugin(name: 'Dev Echo', version: '0.1.0'),
      dev: true,
    );

    final repo = _FakePluginsRepository(
      initialPlugins: [],
      initialReviews: [devReview],
    );

    await tester.pumpWidget(
      _wrap(const ServerPluginsPage(serverKey: _devKey), repository: repo),
    );
    await tester.pumpAndSettle();

    expect(find.text('Dev Echo wants to update from dev mode'), findsOneWidget);
    expect(find.text('Review'), findsOneWidget);

    await tester.tap(find.text('Review'));
    await tester.pumpAndSettle();

    expect(find.byType(ReviewDialog), findsOneWidget);
    expect(find.text('Install Dev Echo'), findsOneWidget);
  });

  testWidgets('install_review_dialog_contents_and_confirmation', (
    tester,
  ) async {
    final review = _sampleReview(
      reviewId: 'rev-install',
      licenseNeedsAcceptance: true,
    );
    final repo = _FakePluginsRepository(
      initialPlugins: [],
      initialReviews: [review],
    );

    await tester.pumpWidget(
      _wrap(
        ReviewDialog(serverKey: _devKey, review: review),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    // Header (§9.1)
    expect(find.text('Install Ticket Master'), findsOneWidget);
    expect(find.text('v1.0.0'), findsOneWidget);
    expect(find.textContaining('by Support Team'), findsOneWidget);
    expect(find.textContaining('Custom Commercial'), findsAtLeast(1));
    expect(find.text('Tickets and support for your server'), findsOneWidget);
    expect(find.text('Unsigned'), findsOneWidget);

    // Unsigned warning
    expect(
      find.text(
        'Unsigned: anyone could have changed this plugin. Install it only if you trust where it came from.',
      ),
      findsOneWidget,
    );

    // Ungrantable permission note
    expect(
      find.text("You don't have this permission yourself"),
      findsOneWidget,
    );

    // Domain list
    expect(find.text('api.support.example.com'), findsOneWidget);

    // Required config field
    expect(find.text('Alert Channel *'), findsOneWidget);

    // Licence checkbox must be accepted before install is enabled
    final installButton = find.widgetWithText(OcButton, 'Install');
    expect(tester.widget<OcButton>(installButton).onPressed, isNull);

    // Accept licence
    final licenceFinder = find.text('I accept the licence');
    await tester.ensureVisible(licenceFinder);
    await tester.pumpAndSettle();
    await tester.tap(licenceFinder);
    await tester.pumpAndSettle();

    // Now install is enabled
    expect(tester.widget<OcButton>(installButton).onPressed, isNotNull);

    await tester.tap(installButton);
    await tester.pumpAndSettle();

    expect(repo.confirmedReviewId, 'rev-install');
  });

  testWidgets('update_review_dialog_contents_and_new_permissions', (
    tester,
  ) async {
    final installed = _samplePlugin(
      id: 'ticket-master',
      name: 'Ticket Master',
      version: '0.9.0',
      author: 'Support Team',
    );
    final updatePlugin = installed.copyWith(version: '1.0.0');

    final review = _sampleReview(
      reviewId: 'rev-update',
      plugin: updatePlugin,
      installed: installed,
      newPermissions: const ['send_messages'],
    );
    final repo = _FakePluginsRepository(
      initialPlugins: [installed],
      initialReviews: [review],
    );

    await tester.pumpWidget(
      _wrap(
        ReviewDialog(serverKey: _devKey, review: review),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    // Header & replace info (§9.1)
    expect(find.text('Update Ticket Master'), findsOneWidget);
    expect(find.text('v1.0.0'), findsOneWidget);
    expect(find.text('by Support Team · MIT'), findsOneWidget);
    expect(
      find.text('This replaces Ticket Master 0.9.0 by Support Team'),
      findsOneWidget,
    );

    // New permission marked with New badge
    expect(find.text('New'), findsOneWidget);

    // Button says Update
    expect(find.widgetWithText(OcButton, 'Update'), findsOneWidget);
  });

  testWidgets('refused_upload_or_confirm_error_shown_in_dialog', (
    tester,
  ) async {
    final review = _sampleReview(reviewId: 'rev-fail');
    final repo = _FakePluginsRepository(
      initialPlugins: [],
      initialReviews: [review],
    )..confirmShouldFail = true;

    await tester.pumpWidget(
      _wrap(
        ReviewDialog(serverKey: _devKey, review: review),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.widgetWithText(OcButton, 'Install'));
    await tester.pumpAndSettle();

    // Failure message shows inside the dialog via InlineError
    expect(find.byType(InlineError), findsOneWidget);
    expect(
      find.text('Signature verification failed on server'),
      findsOneWidget,
    );
  });

  testWidgets('expired_or_externally_confirmed_review_closes_cleanly', (
    tester,
  ) async {
    final review = _sampleReview(reviewId: 'rev-expire');
    final repo = _FakePluginsRepository(
      initialPlugins: [],
      initialReviews: [review],
    );

    var closed = false;

    await tester.pumpWidget(
      _wrap(
        Consumer(
          builder: (context, ref, _) => TextButton(
            onPressed: () async {
              ref
                  .read(pluginReviewsProvider(_devKey).notifier)
                  .apply(PluginReviewUpdated(_devKey, review));
              await showReviewDialog(
                context,
                serverKey: _devKey,
                review: review,
              );
              closed = true;
            },
            child: const Text('Open Dialog'),
          ),
        ),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Open Dialog'));
    await tester.pumpAndSettle();

    expect(find.byType(ReviewDialog), findsOneWidget);

    // Review expires or is discarded/confirmed elsewhere
    await repo.discardPluginReview(_devKey, 'rev-expire');
    await tester.pumpAndSettle();

    // Dialog closed cleanly!
    expect(find.byType(ReviewDialog), findsNothing);
    expect(closed, true);
  });

  testWidgets('review_dialog_stays_open_on_poll_before_review_exists', (
    tester,
  ) async {
    final review = _sampleReview(reviewId: 'rev-poll');
    final repo = _FakePluginsRepository(
      initialPlugins: [],
      initialReviews: [review],
    );

    var closed = false;

    await tester.pumpWidget(
      _wrap(
        Consumer(
          builder: (context, ref, _) => TextButton(
            onPressed: () async {
              await showReviewDialog(
                context,
                serverKey: _devKey,
                review: review,
              );
              closed = true;
            },
            child: const Text('Open Dialog'),
          ),
        ),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Open Dialog'));
    await tester.pumpAndSettle();

    expect(find.byType(ReviewDialog), findsOneWidget);

    // 1. Poll answers with empty list before upload/review exists in provider:
    // Dialog must stay open!
    final container = ProviderScope.containerOf(
      tester.element(find.byType(ReviewDialog)),
    );
    container.read(pluginReviewsProvider(_devKey).notifier).set([]);
    await tester.pumpAndSettle();

    expect(find.byType(ReviewDialog), findsOneWidget);
    expect(closed, false);

    // 2. Add the review to provider
    container
        .read(pluginReviewsProvider(_devKey).notifier)
        .apply(PluginReviewUpdated(_devKey, review));
    await tester.pumpAndSettle();
    expect(find.byType(ReviewDialog), findsOneWidget);

    // 3. Remove the review from provider -> now it closes cleanly
    container
        .read(pluginReviewsProvider(_devKey).notifier)
        .apply(PluginReviewRemoved(_devKey, review.reviewId));
    await tester.pumpAndSettle();

    expect(find.byType(ReviewDialog), findsNothing);
    expect(closed, true);
  });

  testWidgets('a_confirm_that_fails_after_the_removal_event_shows_its_error', (
    tester,
  ) async {
    final review = _sampleReview(
      reviewId: 'rev-fail-removal',
      licenseNeedsAcceptance: false,
    );
    final repo = _FakePluginsRepository(
      initialPlugins: [],
      initialReviews: [review],
    );
    repo.emitRemovalBeforeConfirm = true;
    repo.confirmDelay = const Duration(milliseconds: 50);
    repo.confirmShouldFail = true;

    await tester.pumpWidget(_wrap(_dialogOverPage(review), repository: repo));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open Dialog'));
    await tester.pumpAndSettle();
    await _routeReviewEvents(tester, repo, review);

    // Tap Install
    final installButton = find.widgetWithText(OcButton, 'Install');
    await tester.tap(installButton);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pumpAndSettle();

    // Dialog must stay open and render the error!
    expect(find.byType(ReviewDialog), findsOneWidget);
    expect(
      find.text('Signature verification failed on server'),
      findsOneWidget,
    );
  });

  testWidgets('a_confirm_closes_only_the_review_dialog', (tester) async {
    final review = _sampleReview(
      reviewId: 'rev-success-removal',
      licenseNeedsAcceptance: false,
    );
    final repo = _FakePluginsRepository(
      initialPlugins: [],
      initialReviews: [review],
    );
    repo.emitRemovalBeforeConfirm = true;
    repo.confirmDelay = const Duration(milliseconds: 50);
    repo.confirmShouldFail = false;

    await tester.pumpWidget(_wrap(_dialogOverPage(review), repository: repo));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Open Dialog'));
    await tester.pumpAndSettle();
    expect(find.byType(ReviewDialog), findsOneWidget);
    await _routeReviewEvents(tester, repo, review);

    final installButton = find.widgetWithText(OcButton, 'Install');
    await tester.tap(installButton);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pumpAndSettle();

    // Review dialog closed, but base page remains (did not pop twice!)
    expect(find.byType(ReviewDialog), findsNothing);
    expect(find.text('Open Dialog'), findsOneWidget);
  });

  testWidgets('an_untouched_choice_sends_what_it_shows', (tester) async {
    final plugin = _samplePlugin().copyWith(
      configFields: const [
        PluginConfigField(
          key: 'log_level',
          label: 'Log Level',
          kind: PluginConfigKind.choice,
          choices: ['info', 'debug', 'warn'],
          required: true,
        ),
      ],
    );
    final review = _sampleReview(
      reviewId: 'rev-choice',
      plugin: plugin,
      licenseNeedsAcceptance: false,
    );
    final repo = _FakePluginsRepository(
      initialPlugins: [],
      initialReviews: [review],
    );

    await tester.pumpWidget(
      _wrap(
        ReviewDialog(serverKey: _devKey, review: review),
        repository: repo,
      ),
    );
    await tester.pumpAndSettle();

    // Chip 'info' is displayed as selected without touching it
    expect(find.text('info'), findsOneWidget);

    // Tap Install
    await tester.tap(find.widgetWithText(OcButton, 'Install'));
    await tester.pumpAndSettle();

    // Value sent must be 'info'
    expect(repo.lastConfirmedConfig, isNotNull);
    final sent = repo.lastConfirmedConfig!.firstWhere(
      (c) => c.key == 'log_level',
    );
    expect(sent.value, 'info');
    expect(sent.hasValue, true);
  });

  testWidgets('config_fields_channel_picker_sends_id', (tester) async {
    const data = ServerData(
      self: User(id: 1000, displayName: 'Admin'),
      info: ServerInfo(
        serverIdHex: '01',
        name: 'Test Server',
        everyoneRoleId: 1,
      ),
      channels: {
        101: Channel(
          id: 101,
          kind: ChannelKind.text,
          name: 'general',
          position: 0,
        ),
        102: Channel(
          id: 102,
          kind: ChannelKind.voice,
          name: 'lobby',
          position: 1,
        ),
        103: Channel(
          id: 103,
          kind: ChannelKind.category,
          name: 'Headers',
          position: 2,
        ),
      },
      roles: {},
      members: {},
      serverPermissions: Permissions.managePlugins,
      channelPermissions: {},
    );

    String? updatedKey;
    String? updatedVal;

    await tester.pumpWidget(
      _wrap(
        PluginConfigFields(
          serverKey: _devKey,
          fields: const [
            PluginConfigField(
              key: 'announcements',
              label: 'Announcements Channel',
              kind: PluginConfigKind.channel,
              required: true,
            ),
          ],
          values: const {},
          onChanged: (key, val) {
            updatedKey = key;
            updatedVal = val;
          },
        ),
        repository: _FakePluginsRepository(),
        serverData: data,
      ),
    );
    await tester.pumpAndSettle();

    // Category 'Headers' must NOT be in the picker options
    await tester.tap(find.text('Select channel...'));
    await tester.pumpAndSettle();

    expect(find.text('#general'), findsOneWidget);
    expect(find.text('#lobby'), findsOneWidget);
    expect(find.text('#Headers'), findsNothing);

    await tester.tap(find.text('#general'));
    await tester.pumpAndSettle();

    expect(updatedKey, 'announcements');
    expect(updatedVal, '101');
  });

  testWidgets('config_fields_role_picker_sends_id', (tester) async {
    const data = ServerData(
      self: User(id: 1000, displayName: 'Admin'),
      info: ServerInfo(
        serverIdHex: '01',
        name: 'Test Server',
        everyoneRoleId: 1,
      ),
      channels: {},
      roles: {
        1: Role(
          id: 1,
          name: '@everyone',
          color: 0,
          position: 0,
          permissions: Permissions.all,
        ),
        501: Role(
          id: 501,
          name: 'Moderator',
          color: 0x00FF00,
          position: 1,
          permissions: Permissions.all,
        ),
      },
      members: {},
      serverPermissions: Permissions.managePlugins,
      channelPermissions: {},
    );

    String? updatedKey;
    String? updatedVal;

    await tester.pumpWidget(
      _wrap(
        PluginConfigFields(
          serverKey: _devKey,
          fields: const [
            PluginConfigField(
              key: 'mod_role',
              label: 'Mod Role',
              kind: PluginConfigKind.role,
              required: true,
            ),
          ],
          values: const {},
          onChanged: (key, val) {
            updatedKey = key;
            updatedVal = val;
          },
        ),
        repository: _FakePluginsRepository(),
        serverData: data,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Select role...'));
    await tester.pumpAndSettle();

    expect(find.text('Moderator'), findsOneWidget);

    await tester.tap(find.text('Moderator'));
    await tester.pumpAndSettle();

    expect(updatedKey, 'mod_role');
    expect(updatedVal, '501');
  });

  testWidgets('config_fields_user_picker_sends_id', (tester) async {
    final data = ServerData(
      self: const User(id: 1000, displayName: 'Admin'),
      info: const ServerInfo(
        serverIdHex: '01',
        name: 'Test Server',
        everyoneRoleId: 1,
      ),
      channels: const {},
      roles: const {},
      members: {
        1000: Member(
          user: const User(id: 1000, displayName: 'Kai'),
          joinedAt: DateTime.fromMillisecondsSinceEpoch(0),
          roleIds: const [1],
        ),
        2000: Member(
          user: const User(id: 2000, displayName: 'Alex'),
          joinedAt: DateTime.fromMillisecondsSinceEpoch(0),
          roleIds: const [1],
        ),
      },
      serverPermissions: Permissions.managePlugins,
      channelPermissions: const {},
    );

    String? updatedKey;
    String? updatedVal;

    await tester.pumpWidget(
      _wrap(
        PluginConfigFields(
          serverKey: _devKey,
          fields: const [
            PluginConfigField(
              key: 'owner',
              label: 'Owner User',
              kind: PluginConfigKind.user,
              required: true,
            ),
          ],
          values: const {},
          onChanged: (key, val) {
            updatedKey = key;
            updatedVal = val;
          },
        ),
        repository: _FakePluginsRepository(),
        serverData: data,
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('Select user...'));
    await tester.pumpAndSettle();

    expect(find.text('Alex'), findsOneWidget);

    await tester.tap(find.text('Alex'));
    await tester.pumpAndSettle();

    expect(updatedKey, 'owner');
    expect(updatedVal, '2000');
  });

  testWidgets('a_config_text_field_keeps_its_caret_while_typing', (
    tester,
  ) async {
    final values = <String, String>{'name': 'abc'};

    await tester.pumpWidget(
      _wrap(
        StatefulBuilder(
          builder: (context, setState) => PluginConfigFields(
            serverKey: _devKey,
            fields: const [
              PluginConfigField(
                key: 'name',
                label: 'Name',
                kind: PluginConfigKind.text,
              ),
            ],
            values: values,
            onChanged: (key, val) => setState(() => values[key] = val),
          ),
        ),
        repository: _FakePluginsRepository(),
      ),
    );
    await tester.pumpAndSettle();

    // Type in the middle of the text, as a person would.
    await tester.showKeyboard(find.byType(TextField));
    tester.testTextInput.updateEditingValue(
      const TextEditingValue(
        text: 'aXbc',
        selection: TextSelection.collapsed(offset: 2),
      ),
    );
    await tester.pump();

    expect(values['name'], 'aXbc');
    final controller = tester
        .widget<TextField>(find.byType(TextField))
        .controller!;
    expect(controller.text, 'aXbc');
    expect(controller.selection.baseOffset, 2);
  });

  testWidgets('config_fields_boolean_choice_and_number', (tester) async {
    final values = <String, String>{'enabled': 'false', 'threshold': '42'};

    await tester.pumpWidget(
      _wrap(
        StatefulBuilder(
          builder: (context, setState) => PluginConfigFields(
            serverKey: _devKey,
            fields: const [
              PluginConfigField(
                key: 'enabled',
                label: 'Enable Feature',
                kind: PluginConfigKind.boolean,
              ),
              PluginConfigField(
                key: 'threshold',
                label: 'Threshold',
                kind: PluginConfigKind.number,
              ),
            ],
            values: values,
            onChanged: (key, val) => setState(() => values[key] = val),
          ),
        ),
        repository: _FakePluginsRepository(),
      ),
    );
    await tester.pumpAndSettle();

    // Toggle boolean switch
    await tester.tap(find.byType(OcSwitch));
    await tester.pumpAndSettle();
    expect(values['enabled'], 'true');

    // Type invalid number
    await tester.enterText(find.byType(OcTextField), 'not_a_number');
    await tester.pumpAndSettle();
    expect(values['threshold'], 'not_a_number');
    expect(find.text('Must be a valid number'), findsOneWidget);
  });

  for (final (name, colors) in themes) {
    testWidgets('plugins list in $name', (tester) async {
      final running = _samplePlugin(
        id: 'welcome-bot',
        name: 'Welcome Bot',
        version: '0.1.0',
        author: 'Amin',
        state: PluginState.running,
        signature: PluginSignature.signed,
        githubCheck: GithubCheck.verified,
        cpuPermille: 15,
        memoryBytes: 32 * 1024 * 1024,
      );

      final repo = _FakePluginsRepository(initialPlugins: [running]);

      await tester.pumpWidget(
        _wrap(
          const ServerPluginsPage(serverKey: _devKey),
          repository: repo,
          colors: colors,
        ),
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(ServerPluginsPage),
        matchesGoldenFile('goldens/plugins_tab_$name.png'),
      );
    });

    testWidgets('review dialog in $name', (tester) async {
      tester.view.physicalSize = const Size(800, 1000);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() => tester.view.resetPhysicalSize());

      final review = _sampleReview(
        reviewId: 'rev-golden',
        licenseNeedsAcceptance: true,
      );
      final repo = _FakePluginsRepository(
        initialPlugins: [],
        initialReviews: [review],
      );

      await tester.pumpWidget(
        _wrap(
          ReviewDialog(serverKey: _devKey, review: review),
          repository: repo,
          colors: colors,
        ),
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(ReviewDialog),
        matchesGoldenFile('goldens/review_dialog_$name.png'),
      );
    });

    testWidgets('review dialog narrow in $name', (tester) async {
      tester.view.physicalSize = const Size(540, 1000);
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() => tester.view.resetPhysicalSize());

      final review = _sampleReview(
        reviewId: 'rev-golden-narrow',
        licenseNeedsAcceptance: true,
      );
      final repo = _FakePluginsRepository(
        initialPlugins: [],
        initialReviews: [review],
      );

      await tester.pumpWidget(
        _wrap(
          ReviewDialog(serverKey: _devKey, review: review),
          repository: repo,
          colors: colors,
        ),
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(ReviewDialog),
        matchesGoldenFile('goldens/review_dialog_narrow_$name.png'),
      );
    });
  }
}
