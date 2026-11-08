import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/repository/repository.dart';

/// The installed plugins of a server, kept in step with the server's events.
class PluginsNotifier extends AsyncNotifier<List<Plugin>> {
  PluginsNotifier(this.serverKey);

  final String serverKey;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  @override
  FutureOr<List<Plugin>> build() {
    try {
      return _repository.fetchPlugins(serverKey);
    } catch (_) {
      return const [];
    }
  }

  void set(List<Plugin> plugins) => state = AsyncData(plugins);

  void apply(RepoEvent event) {
    switch (event) {
      case PluginUpdated(:final plugin):
        final current = state.value ?? const [];
        final index = current.indexWhere((p) => p.id == plugin.id);
        if (index >= 0) {
          final copy = List<Plugin>.of(current);
          copy[index] = plugin;
          state = AsyncData(copy);
        } else {
          state = AsyncData([...current, plugin]);
        }
      case PluginRemoved(:final pluginId):
        final current = state.value ?? const [];
        state = AsyncData([for (final p in current) if (p.id != pluginId) p]);
      default:
        break;
    }
  }
}

final pluginsProvider =
    AsyncNotifierProvider.family<PluginsNotifier, List<Plugin>, String>(
      PluginsNotifier.new,
    );

/// The plugin users (bots) of a server, by user id.
class PluginUsersNotifier extends Notifier<Map<int, PluginUser>> {
  PluginUsersNotifier(this.serverKey);

  final String serverKey;

  @override
  Map<int, PluginUser> build() => const {};

  void apply(RepoEvent event) {
    switch (event) {
      case Ready(:final snapshot):
        state = {for (final u in snapshot.pluginUsers) u.userId: u};
      case PluginUsersUpdated(:final users):
        state = {for (final u in users) u.userId: u};
      case PluginRemoved(:final pluginId):
        state = {
          for (final entry in state.entries)
            entry.key: entry.value.pluginId == pluginId
                ? PluginUser(
                    userId: entry.value.userId,
                    pluginId: entry.value.pluginId,
                    name: entry.value.name,
                    avatarHash: entry.value.avatarHash,
                    installed: false,
                  )
                : entry.value,
        };
      default:
        break;
    }
  }
}

final pluginUsersProvider =
    NotifierProvider.family<PluginUsersNotifier, Map<int, PluginUser>, String>(
      PluginUsersNotifier.new,
    );

/// Reviews waiting for review on a server, by review id.
class PluginReviewsNotifier extends Notifier<Map<String, PluginReview>> {
  PluginReviewsNotifier(this.serverKey);

  final String serverKey;

  @override
  Map<String, PluginReview> build() => const {};

  void apply(RepoEvent event) {
    switch (event) {
      case PluginReviewUpdated(:final review):
        state = {...state, review.reviewId: review};
      case PluginReviewRemoved(:final reviewId):
        state = {...state}..remove(reviewId);
      default:
        break;
    }
  }
}

final pluginReviewsProvider =
    NotifierProvider.family<
      PluginReviewsNotifier,
      Map<String, PluginReview>,
      String
    >(PluginReviewsNotifier.new);
