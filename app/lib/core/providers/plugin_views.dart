import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/logs/app_log.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/src/rust/api/logs.dart' show LogLevel;
import 'package:opencord_blocks/opencord_blocks.dart';

/// A [BlocksController] that also exposes the real backend [viewId] once open,
/// and tells its view `pause` and `resume`.
class PluginBlocksController extends BlocksController {
  final ValueNotifier<String?> viewId = ValueNotifier<String?>(null);

  /// The areas drawing the view now, and those of them that hold it paused
  /// (their window hidden, or the area's `paused`).
  final _areas = <Object>{};
  final _pausedBy = <Object>{};

  /// Kept running while no area draws it (an opened activity while its
  /// page is elsewhere): then it's paused while none does, once one has.
  bool _keptWithoutArea = false;
  bool _drawn = false;

  /// What the view was last told: `pause` (true) or `resume`. A view starts
  /// running.
  bool _toldPaused = false;
  bool _disposed = false;

  /// [area] draws the view now.
  void attachArea(Object area) {
    _areas.add(area);
    _drawn = true;
    _tellPaused();
  }

  /// [area] no longer draws the view. A view nothing keeps goes with its
  /// area, so it's told nothing more.
  void detachArea(Object area) {
    _areas.remove(area);
    _pausedBy.remove(area);
    if (_keptWithoutArea) _tellPaused();
  }

  /// Whether [area] holds the view paused.
  void setPausedBy(Object area, {required bool paused}) {
    if (paused) {
      _pausedBy.add(area);
    } else {
      _pausedBy.remove(area);
    }
    _tellPaused();
  }

  /// The view goes on while no area draws it, paused.
  void keepWithoutArea() {
    if (_keptWithoutArea) return;
    _keptWithoutArea = true;
    _tellPaused();
  }

  /// Tells the view `pause` or `resume` when whether it should run changed
  /// since it was last told: once per change, whatever caused it.
  void _tellPaused() {
    if (_disposed) return;
    final pause =
        _pausedBy.isNotEmpty || (_keptWithoutArea && _drawn && _areas.isEmpty);
    if (pause == _toldPaused) return;
    _toldPaused = pause;
    send({'kind': 'lifecycle', 'event': pause ? 'pause' : 'resume'});
  }

  @override
  void dispose() {
    _disposed = true;
    viewId.dispose();
    super.dispose();
  }
}

/// Where a plugin view opens: a plugin channel, or an activity session in a
/// call (plugins M3, §15).
@immutable
sealed class ViewPlaceRef {
  const ViewPlaceRef();

  String get server;
}

/// A plugin channel's view.
final class ChannelPlace extends ViewPlaceRef {
  const ChannelPlace(this.server, this.channel);

  @override
  final String server;
  final int channel;

  @override
  bool operator ==(Object other) =>
      other is ChannelPlace &&
      other.server == server &&
      other.channel == channel;

  @override
  int get hashCode => Object.hash(ChannelPlace, server, channel);

  @override
  String toString() => 'ChannelPlace($server, $channel)';
}

/// An activity session's view. Equal by server and session id only: the
/// session's viewers change while its view is open, and that's still the
/// same place.
final class ActivityPlace extends ViewPlaceRef {
  const ActivityPlace(this.server, this.session);

  @override
  final String server;
  final ActivitySession session;

  @override
  bool operator ==(Object other) =>
      other is ActivityPlace &&
      other.server == server &&
      other.session.id == session.id;

  @override
  int get hashCode => Object.hash(ActivityPlace, server, session.id);

  @override
  String toString() => 'ActivityPlace($server, ${session.id})';
}

/// One plugin view's controller: connects its renderer to the server and
/// closes it when the page leaves.
final pluginViewProvider = Provider.autoDispose
    .family<PluginBlocksController, ViewPlaceRef>((ref, place) {
      final repository = ref.watch(repositoryProvider);
      final controller = PluginBlocksController();

      StreamSubscription<ViewOutput>? outputSub;
      StreamSubscription<String>? inputSub;
      String? openViewId;
      var disposed = false;

      ref.onDispose(() {
        disposed = true;
        outputSub?.cancel();
        inputSub?.cancel();
        controller.dispose();
        if (openViewId != null) {
          unawaited(
            repository
                .closeView(openViewId!)
                .catchError(
                  (Object e) => appLog('close view failed: $e', name: 'views'),
                ),
          );
        }
      });

      () async {
        final pendingInputs = <String>[];
        final contextCompleter = Completer<String>();
        // Outputs can reach the stream before openView returns the id to
        // match them by, so they're held while the open is in flight, and
        // only then: a failed open stops listening and lets them go.
        final earlyOutputs = <ViewOutput>[];

        inputSub = controller.inputs.listen((json) {
          if (openViewId != null) {
            repository.viewInput(openViewId!, json);
          } else {
            if (!contextCompleter.isCompleted) {
              try {
                final decoded = jsonDecode(json);
                if (decoded is Map && decoded['kind'] == 'context') {
                  contextCompleter.complete(json);
                  return;
                }
              } on FormatException catch (error) {
                appLog(
                  'Malformed view input JSON before view open: $error',
                  name: 'views',
                  level: LogLevel.warn,
                );
              }
            }
            pendingInputs.add(json);
          }
        });

        try {
          final contextJson = await contextCompleter.future;
          if (disposed) return;

          outputSub = repository.viewOutputs.listen((output) {
            final viewId = openViewId;
            if (viewId == null) {
              earlyOutputs.add(output);
            } else if (output.viewId == viewId) {
              controller.applyBatch(output.json);
            }
          });

          final viewId = await switch (place) {
            ChannelPlace(:final server, :final channel) => repository.openView(
              server,
              channel,
              contextJson,
            ),
            ActivityPlace(:final server, :final session) =>
              repository.openActivityView(server, session, contextJson),
          };
          if (disposed) {
            unawaited(
              repository
                  .closeView(viewId)
                  .catchError(
                    (Object e) =>
                        appLog('close view failed: $e', name: 'views'),
                  ),
            );
            return;
          }
          openViewId = viewId;
          controller.viewId.value = viewId;

          for (final early in earlyOutputs) {
            if (early.viewId == viewId) {
              controller.applyBatch(early.json);
            }
          }
          earlyOutputs.clear();

          for (final pending in pendingInputs) {
            repository.viewInput(viewId, pending);
          }
          pendingInputs.clear();
        } on RepoException catch (e) {
          await outputSub?.cancel();
          outputSub = null;
          earlyOutputs.clear();
          appLog('open view failed: $e', name: 'views');
          if (!disposed) {
            controller.reportError(e.message);
          }
        }
      }();

      return controller;
    });

/// A reference to a plugin asset file: server, plugin and file path.
@immutable
class PluginAssetRef {
  const PluginAssetRef({
    required this.serverKey,
    required this.pluginId,
    required this.path,
    this.sha256 = '',
  });

  final String serverKey;
  final String pluginId;
  final String path;
  final String sha256;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is PluginAssetRef &&
          other.serverKey == serverKey &&
          other.pluginId == pluginId &&
          other.path == path &&
          other.sha256 == sha256;

  @override
  int get hashCode => Object.hash(serverKey, pluginId, path, sha256);
}

/// Loads a plugin asset onto disk and yields its local path.
final pluginAssetFileProvider = FutureProvider.autoDispose
    .family<String, PluginAssetRef>((ref, asset) async {
      if (asset.path.isEmpty) return '';
      final repository = ref.watch(repositoryProvider);
      return repository.pluginAssetFile(
        asset.serverKey,
        asset.pluginId,
        asset.path,
        asset.sha256,
      );
    });
