import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/logs/app_log.dart';
import 'package:opencord/core/providers/providers.dart' show ChannelRef;
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/src/rust/api/logs.dart' show LogLevel;
import 'package:opencord_blocks/opencord_blocks.dart';

/// One plugin view's controller: connects its renderer to the server and
/// closes it when the page leaves.
final pluginViewProvider = Provider.autoDispose
    .family<BlocksController, ChannelRef>((ref, channel) {
      final repository = ref.watch(repositoryProvider);
      final controller = BlocksController();

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

          final viewId = await repository.openView(
            channel.server,
            channel.channel,
            contextJson,
          );
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
