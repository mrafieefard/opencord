import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/providers/plugins_state.dart';
import 'package:opencord/core/repository/events.dart';

const _server = 'opencord.example:7710';

Plugin _plugin(
  String id,
  String name, {
  PluginState state = PluginState.running,
}) => Plugin(
  id: id,
  name: name,
  version: '1.0.0',
  author: 'Author',
  description: 'A test plugin',
  signature: PluginSignature.unsigned,
  githubCheck: GithubCheck.unspecified,
  state: state,
  statusReason: '',
  permissions: const [],
  domains: const [],
  configFields: const [],
  config: const [],
  userId: 101,
  roleId: 202,
  installedAt: DateTime(2026),
  updatedAt: DateTime(2026),
  cpuPermille: 0,
  memoryBytes: 0,
  databaseBytes: 0,
  filesBytes: 0,
);

void main() {
  test('a_plugin_update_replaces_the_plugin_once', () {
    final container = ProviderContainer();
    addTearDown(container.dispose);

    final plugins = container.read(pluginsProvider(_server).notifier);

    plugins.apply(PluginUpdated(_server, _plugin('bot', 'Bot 1.0')));
    expect(container.read(pluginsProvider(_server)).value?.length, 1);
    expect(
      container.read(pluginsProvider(_server)).value?.first.name,
      'Bot 1.0',
    );

    // Update with new name
    plugins.apply(PluginUpdated(_server, _plugin('bot', 'Bot 2.0')));
    final list = container.read(pluginsProvider(_server)).value;
    expect(list?.length, 1, reason: 'must replace, not append');
    expect(list?.first.name, 'Bot 2.0');

    // Add second plugin
    plugins.apply(PluginUpdated(_server, _plugin('tickets', 'Tickets')));
    expect(container.read(pluginsProvider(_server)).value?.length, 2);

    // Remove first plugin
    plugins.apply(const PluginRemoved(_server, 'bot'));
    final afterRemove = container.read(pluginsProvider(_server)).value;
    expect(afterRemove?.length, 1);
    expect(afterRemove?.first.id, 'tickets');
  });
}
