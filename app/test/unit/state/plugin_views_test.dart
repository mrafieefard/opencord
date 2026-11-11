import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/providers/plugin_views.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/core_mapping.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/rust/rust_repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

import '../../support/fake_core.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  // Fails if core plugin channel or state cannot be mapped to model, or Ready ignores pluginChannelTypes.
  test('channel_kind_and_state_map_from_the_core', () {
    const coreChannel = core.Channel(
      id: 42,
      kind: core.ChannelKind.plugin,
      name: 'tickets',
      topic: 'Get help here',
      parentId: 10,
      position: 3,
      overwrites: [],
      bitrate: 64000,
      userLimit: 0,
      textInVoice: true,
      pluginId: 'support-desk',
      typeKey: 'tickets',
      pluginState: core.PluginChannelState.available,
    );

    final channel = channelFrom(coreChannel);
    expect(channel.kind, ChannelKind.plugin);
    expect(channel.pluginId, 'support-desk');
    expect(channel.typeKey, 'tickets');
    expect(channel.pluginState, PluginChannelState.available);
    expect(channel.isAvailablePlugin, isTrue);
    expect(channel.isPlugin, isTrue);

    const stoppedCore = core.Channel(
      id: 43,
      kind: core.ChannelKind.plugin,
      name: 'archive',
      parentId: null,
      position: 4,
      overwrites: [],
      bitrate: 64000,
      userLimit: 0,
      textInVoice: true,
      pluginId: 'support-desk',
      typeKey: 'tickets',
      pluginState: core.PluginChannelState.pluginStopped,
    );
    final stopped = channelFrom(stoppedCore);
    expect(stopped.kind, ChannelKind.plugin);
    expect(stopped.pluginState, PluginChannelState.pluginStopped);
    expect(stopped.isAvailablePlugin, isFalse);
    expect(stopped.isPlugin, isTrue);

    // Ready snapshot keeps plugin channels and pluginChannelTypes
    final ready = snapshotFrom(
      core.ReadySnapshot(
        selfUser: const core.User(
          id: 1,
          publicKeyHex: 'aa',
          fingerprint: 'fp',
          displayName: 'Alex',
          avatarHash: null,
        ),
        server: const core.ServerInfo(
          serverIdHex: '01',
          name: 'Opencord',
          description: '',
          ownerId: 1,
          everyoneRoleId: 1,
          openJoin: false,
        ),
        channels: const [coreChannel],
        roles: const [],
        members: const [],
        presences: const [],
        serverPermissions: 0,
        channelPermissions: const [],
        voiceStates: const [],
        streams: const [],
        voiceEnabled: false,
        voiceSettings: coreVoiceSettings,
        soundboardSounds: const [],
        addresses: const core.ServerAddresses(domains: []),
        commands: const core.CommandList(commands: [], resultsPublic: true),
        uploadSettings: coreUploadSettings,
        privateMessages: false,
        privateSettings: const core.PrivateSettings(
          enabled: false,
          friendsOnly: false,
          allowedByConfig: false,
        ),
        pluginUsers: const [],
        pluginChannelTypes: const [
          core.PluginChannelType(
            pluginId: 'support-desk',
            typeKey: 'tickets',
            name: 'Tickets',
            description: 'Support desk tickets',
            icon: 'assets/icon.png',
            pluginName: 'Support Desk',
          ),
        ],
      ),
    );

    expect(ready.channels, hasLength(1));
    expect(ready.channels.first.kind, ChannelKind.plugin);
    expect(ready.pluginChannelTypes, hasLength(1));
    expect(ready.pluginChannelTypes.first.pluginId, 'support-desk');
    expect(ready.pluginChannelTypes.first.typeKey, 'tickets');
    expect(ready.pluginChannelTypes.first.name, 'Tickets');
  });

  // Fails if output for one viewId is routed to another view controller.
  test(
    'a_view_output_reaches_only_its_controller (two views open: road core stream -> provider -> controller)',
    () async {
      final fakeCore = FakeCoreApi();
      final repository = await RustRepository.open(
        core: fakeCore,
        identities: MemoryIdentityStore(),
        store: MemoryKeyValueStore(),
      );
      repository.start();
      addTearDown(repository.dispose);

      final container = ProviderContainer(
        overrides: [repositoryProvider.overrideWithValue(repository)],
      );
      addTearDown(container.dispose);

      fakeCore.viewOpenHandler = (serverKey, channelId, contextJson) async {
        return channelId == 10 ? 'view_1' : 'view_2';
      };

      final sub1 = container.listen(
        pluginViewProvider((server: 'srv1', channel: 10)),
        (_, _) {},
      );
      final sub2 = container.listen(
        pluginViewProvider((server: 'srv1', channel: 20)),
        (_, _) {},
      );
      final controller1 = sub1.read();
      final controller2 = sub2.read();

      controller1.send({'kind': 'context', 'width': 800.0, 'height': 600.0});
      controller2.send({'kind': 'context', 'width': 800.0, 'height': 600.0});

      // Wait for openView to complete
      await pumpEventQueue();

      final batch1 = jsonEncode({
        'view': 'view_1',
        'seq': 1,
        'ops': [
          {
            'op': 'create',
            'id': 1,
            'block': 'box',
            'props': <String, Object?>{},
            'events': <String>[],
            'children': <int>[],
          },
          {'op': 'roots', 'main': 1, 'modals': <Object?>[]},
        ],
      });
      final batch2 = jsonEncode({
        'view': 'view_2',
        'seq': 1,
        'ops': [
          {
            'op': 'create',
            'id': 2,
            'block': 'box',
            'props': <String, Object?>{},
            'events': <String>[],
            'children': <int>[],
          },
          {'op': 'roots', 'main': 2, 'modals': <Object?>[]},
        ],
      });

      fakeCore.emitViewOutput(core.ViewOutput(viewId: 'view_1', json: batch1));
      fakeCore.emitViewOutput(core.ViewOutput(viewId: 'view_2', json: batch2));

      await pumpEventQueue();

      // Controller 1 receives batch 1, NOT batch 2
      expect(controller1.node(1), isNotNull);
      expect(controller1.node(2), isNull);
      expect(controller1.seq, 1);

      // Controller 2 receives batch 2, NOT batch 1
      expect(controller2.node(2), isNotNull);
      expect(controller2.node(1), isNull);
      expect(controller2.seq, 1);
    },
  );

  // Fails if disposing the provider subscription does not close the view in repository and core.
  test('closing_the_provider_closes_the_view', () async {
    final fakeCore = FakeCoreApi();
    final repository = await RustRepository.open(
      core: fakeCore,
      identities: MemoryIdentityStore(),
      store: MemoryKeyValueStore(),
    );
    repository.start();
    addTearDown(repository.dispose);

    final container = ProviderContainer(
      overrides: [repositoryProvider.overrideWithValue(repository)],
    );

    fakeCore.viewOpenHandler = (serverKey, channelId, contextJson) async =>
        'view_99';

    final sub = container.listen(
      pluginViewProvider((server: 'srv1', channel: 10)),
      (_, _) {},
    );
    sub.read().send({'kind': 'context', 'width': 800.0, 'height': 600.0});
    await pumpEventQueue();

    expect(fakeCore.closedViews, isEmpty);

    // Closing the subscription disposes the autoDispose provider
    sub.close();
    await pumpEventQueue();

    expect(fakeCore.closedViews, contains('view_99'));
  });

  // Fails if viewOpen does not receive the area's width and context from BlocksView,
  // or if inputs before open are dropped.
  test('view_open_receives_context_and_queues_early_inputs', () async {
    final fakeCore = FakeCoreApi();
    final repository = await RustRepository.open(
      core: fakeCore,
      identities: MemoryIdentityStore(),
      store: MemoryKeyValueStore(),
    );
    repository.start();
    addTearDown(repository.dispose);

    final container = ProviderContainer(
      overrides: [repositoryProvider.overrideWithValue(repository)],
    );
    addTearDown(container.dispose);

    String? capturedContext;
    fakeCore.viewOpenHandler = (serverKey, channelId, contextJson) async {
      capturedContext = contextJson;
      return 'view_ctx_1';
    };

    final sub = container.listen(
      pluginViewProvider((server: 'srv1', channel: 10)),
      (_, _) {},
    );
    final controller = sub.read();

    // Send an early input before context
    controller.send({'kind': 'early', 'data': 123});

    // Send context input as BlocksView does on layout
    controller.send({
      'kind': 'context',
      'width': 800.0,
      'height': 600.0,
      'theme': 'dark',
      'input': 'mouse',
      'safe_area': {'top': 0, 'right': 0, 'bottom': 0, 'left': 0},
      'text_scale': 1.0,
    });

    await pumpEventQueue();

    expect(capturedContext, isNotNull);
    final decoded = jsonDecode(capturedContext!) as Map<String, Object?>;
    expect(decoded['width'], 800.0);
    expect(
      fakeCore.viewInputs.any(
        (i) => i.viewId == 'view_ctx_1' && i.json.contains('"data":123'),
      ),
      isTrue,
    );
  });

  // Fails if repository openView error is swallowed instead of setting controller error state.
  test('open_failure_sets_controller_error', () async {
    final fakeCore = FakeCoreApi();
    final repository = await RustRepository.open(
      core: fakeCore,
      identities: MemoryIdentityStore(),
      store: MemoryKeyValueStore(),
    );
    repository.start();
    addTearDown(repository.dispose);

    final container = ProviderContainer(
      overrides: [repositoryProvider.overrideWithValue(repository)],
    );
    addTearDown(container.dispose);

    fakeCore.viewOpenHandler = (serverKey, channelId, contextJson) async {
      throw const core.CoreError_InvalidInput(message: 'channel not found');
    };

    final sub = container.listen(
      pluginViewProvider((server: 'srv1', channel: 10)),
      (_, _) {},
    );
    final controller = sub.read();

    controller.send({'kind': 'context', 'width': 800.0, 'height': 600.0});
    await pumpEventQueue();

    expect(controller.error, isNotNull);
    expect(controller.error!.message, 'channel not found');
  });

  // Fails if the mock repository initial batch does not reach the controller through the provider,
  // or if counter press does not update the count.
  test('the_mock_counter_counts', () async {
    final mockRepo = MockRepository();
    final container = ProviderContainer(
      overrides: [repositoryProvider.overrideWithValue(mockRepo)],
    );
    addTearDown(container.dispose);

    final sub = container.listen(
      pluginViewProvider((server: 'opencord.example:7710', channel: 10008)),
      (_, _) {},
    );
    final controller = sub.read();

    controller.send({'kind': 'context', 'width': 800.0, 'height': 600.0});
    await pumpEventQueue();

    // Initial batch received through provider into controller
    expect(controller.roots, isNotNull);
    expect(controller.seq, 1);
    final textNode = controller.nodes.firstWhere((n) => n.block == 'text');
    expect(textNode.props['text'], 'Count: 0');

    // Press the button
    controller.event(3, 'press');
    await pumpEventQueue();

    expect(controller.seq, 2);
    expect(controller.node(textNode.id)?.props['text'], 'Count: 1');
  });

  // Fails if outputs emitted by core before viewOpen returns are lost or not delivered to controller.
  test('outputs_emitted_before_view_open_returns_are_delivered', () async {
    final fakeCore = FakeCoreApi();
    final repository = await RustRepository.open(
      core: fakeCore,
      identities: MemoryIdentityStore(),
      store: MemoryKeyValueStore(),
    );
    repository.start();
    addTearDown(repository.dispose);

    final container = ProviderContainer(
      overrides: [repositoryProvider.overrideWithValue(repository)],
    );
    addTearDown(container.dispose);

    final earlyBatch = jsonEncode({
      'view': 'early_view',
      'seq': 1,
      'ops': [
        {
          'op': 'create',
          'id': 1,
          'block': 'box',
          'props': <String, Object?>{},
          'events': <String>[],
          'children': <int>[],
        },
        {'op': 'roots', 'main': 1, 'modals': <Object?>[]},
      ],
    });

    fakeCore.viewOpenHandler = (serverKey, channelId, contextJson) async {
      fakeCore.emitViewOutput(
        core.ViewOutput(viewId: 'early_view', json: earlyBatch),
      );
      return 'early_view';
    };

    final sub = container.listen(
      pluginViewProvider((server: 'srv1', channel: 10)),
      (_, _) {},
    );
    final controller = sub.read();
    controller.send({'kind': 'context', 'width': 800.0, 'height': 600.0});
    await pumpEventQueue();

    expect(controller.node(1), isNotNull);
    expect(controller.roots, isNotNull);
    expect(controller.seq, 1);
  });
}
