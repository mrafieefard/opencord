import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart' show FontLoader;

import 'host.dart';
import 'node.dart';

/// One view's state on the renderer's side: the nodes the core's batches
/// built, what is drawn, the quick dialogs waiting, and the inputs going
/// back. One controller per open view.
///
/// When entries leave: a node on its `drop` op (or [dispose]); a dialog when
/// answered, or when the view stops (`error`, `closed`); a side effect when
/// a [BlocksView] takes it, or [dispose].
class BlocksController extends ChangeNotifier {
  /// A side effect waits only while no view takes it; past this many, the
  /// oldest goes (the view is gone, and so is its point).
  static const maxWaitingEffects = 100;

  /// How long [askBack] waits for the plugin before back goes on as not
  /// handled: a stuck plugin never traps the viewer.
  static const backTimeout = Duration(milliseconds: 500);

  final _nodes = <int, ValueNotifier<BlockNode?>>{};
  final _inputs = StreamController<String>();
  final _effects = <BlocksEffect>[];
  final _dialogs = <BlocksDialog>[];
  final _loadedFonts = <String, String>{};
  final _loadingFonts = <String, Future<String?>>{};
  var _seq = 0;
  BlocksRoots? _roots;
  var _framesOn = false;
  var _backHandler = false;
  var _backIds = 0;
  ({int id, Completer<bool> answer, Timer timeout})? _backWaiting;
  BlocksError? _error;
  var _tooOld = false;
  String? _closed;
  var _disposed = false;

  /// Inputs for the core, as JSON: listened to once, by whoever feeds the
  /// core; inputs sent before then wait.
  Stream<String> get inputs => _inputs.stream;

  /// The last batch applied.
  int get seq => _seq;

  BlockNode? node(int id) => _nodes[id]?.value;

  /// The node as it changes; it becomes null when the node is dropped. Null
  /// for a node the store doesn't hold.
  ValueListenable<BlockNode?>? watch(int id) => _nodes[id];

  Iterable<BlockNode> get nodes => [
    for (final slot in _nodes.values) ?slot.value,
  ];

  BlocksRoots? get roots => _roots;
  List<BlocksDialog> get dialogs => List.unmodifiable(_dialogs);
  bool get framesOn => _framesOn;

  /// Whether the plugin set `app.on_back`.
  bool get backHandler => _backHandler;
  BlocksError? get error => _error;
  bool get tooOld => _tooOld;

  /// Why the server closed the view (`plugin_stopped`, `lost_access`,
  /// `channel_deleted`, `unavailable`, and for an activity's view
  /// `activity_ended` and `left_call`), or null while it's open.
  String? get closed => _closed;

  /// Applies one batch's ops in order and tells listeners once. A batch whose
  /// seq isn't above the last is ignored. A batch that can't be read changes
  /// nothing, is reported to Flutter's error handler (the app's log) without
  /// its content, and stops the view with an error.
  void applyBatch(String json) {
    if (_disposed) return;
    final int seq;
    final List<_Op> ops;
    try {
      (seq, ops) = _read(json);
    } on _Broken catch (broken, stack) {
      _fail(broken, stack);
      notifyListeners();
      return;
    }
    if (seq <= _seq) return;
    _seq = seq;
    try {
      for (final op in ops) {
        op.apply(this);
      }
    } on _Broken catch (broken, stack) {
      _fail(broken, stack);
    }
    notifyListeners();
  }

  /// Sends one input to the core. Inputs after [dispose] are dropped: a
  /// widget being removed may still report (a hover ending).
  void send(Map<String, Object?> input) {
    if (_disposed) return;
    _inputs.add(jsonEncode(input));
  }

  /// Sends a block's event, only when its node handles it now.
  void event(int node, String event, [Map<String, Object?> data = const {}]) {
    if (!(this.node(node)?.handles(event) ?? false)) return;
    send({'kind': 'event', 'node': node, 'event': event, 'data': data});
  }

  /// The phone's back once no overlay is open: asks the plugin's
  /// `app.on_back` and completes with whether it went back itself. False at
  /// once with no handler or a stopped view, and after [backTimeout] with no
  /// answer. An ask while one waits shares it.
  Future<bool> askBack() {
    final waiting = _backWaiting;
    if (waiting != null) return waiting.answer.future;
    if (!_backHandler ||
        _disposed ||
        _error != null ||
        _closed != null ||
        _tooOld) {
      return Future.value(false);
    }
    final id = ++_backIds;
    final answer = Completer<bool>();
    _backWaiting = (
      id: id,
      answer: answer,
      timeout: Timer(backTimeout, () => _answerBack(id, false)),
    );
    send({'kind': 'back', 'id': id});
    return answer.future;
  }

  /// Completes the waiting back `id`; another id (a late answer) is ignored.
  void _answerBack(int id, bool handled) {
    final waiting = _backWaiting;
    if (waiting == null || waiting.id != id) return;
    _backWaiting = null;
    waiting.timeout.cancel();
    waiting.answer.complete(handled);
  }

  /// Answers a quick dialog: `true`/`false` for confirm, the text or null for
  /// prompt, null for alert. A dialog that isn't waiting is ignored.
  void answerDialog(int id, Object? result) {
    final index = _dialogs.indexWhere((dialog) => dialog.id == id);
    if (index < 0) return;
    _dialogs.removeAt(index);
    send({'kind': 'dialog', 'id': id, 'result': result});
    notifyListeners();
  }

  /// Stops the view and sets an error message to display.
  void reportError(String message, [String? details]) {
    if (_disposed) return;
    _stop(BlocksError(message, details: details));
    notifyListeners();
  }

  /// The side effects waiting, oldest first; they're the caller's now.
  List<BlocksEffect> takeEffects() {
    final taken = List.of(_effects);
    _effects.clear();
    return taken;
  }

  /// Loads a custom font for this view once; scoped so two plugins' fonts don't collide.
  Future<String?> loadFont(String font, BlocksHost host) {
    if (_disposed) return Future.value(null);
    if (_loadedFonts.containsKey(font)) return Future.value(_loadedFonts[font]);
    if (_loadingFonts.containsKey(font)) return _loadingFonts[font]!;

    final future = () async {
      try {
        final data = await host.font(font);
        if (data == null || _disposed) return null;
        final family = 'view_${identityHashCode(this)}_$font';
        final loader = FontLoader(family);
        loader.addFont(Future.value(data));
        await loader.load();
        _loadedFonts[font] = family;
        return family;
      } on Object catch (error, stack) {
        FlutterError.reportError(
          FlutterErrorDetails(
            exception: error,
            stack: stack,
            library: 'opencord_blocks',
            context: ErrorDescription('loading font $font'),
          ),
        );
        return null;
      } finally {
        _loadingFonts.remove(font);
      }
    }();

    _loadingFonts[font] = future;
    return future;
  }

  String? resolvedFont(String font) => _loadedFonts[font];

  @override
  void dispose() {
    _disposed = true;
    final waiting = _backWaiting;
    if (waiting != null) _answerBack(waiting.id, false);
    // The node notifiers aren't disposed: widgets for them may still be
    // unsubscribing in the frame that removes them. Nothing else holds them.
    for (final timer in _exitSafetyTimers.values) {
      timer.cancel();
    }
    _exitSafetyTimers.clear();
    _retainedForExit.clear();
    _nodes.clear();
    _effects.clear();
    _dialogs.clear();
    _loadedFonts.clear();
    _loadingFonts.clear();
    unawaited(_inputs.close());
    super.dispose();
  }

  void _fail(_Broken broken, StackTrace stack) {
    FlutterError.reportError(
      FlutterErrorDetails(
        exception: broken,
        stack: stack,
        library: 'opencord_blocks',
        context: ErrorDescription('while applying a plugin view batch'),
      ),
    );
    _stop(const BlocksError('This plugin hit an error'));
  }

  void _stop(BlocksError? error) {
    if (error != null) _error = error;
    _dialogs.clear();
    _framesOn = false;
    _backHandler = false;
    final waiting = _backWaiting;
    if (waiting != null) _answerBack(waiting.id, false);
  }

  ValueNotifier<BlockNode?> _slot(int id) =>
      _nodes[id] ?? (throw _Broken('no node $id'));

  void _create(BlockNode node) {
    if (_nodes.containsKey(node.id)) {
      throw _Broken('node ${node.id} made twice');
    }
    for (final child in node.children) {
      if (!_nodes.containsKey(child)) {
        throw _Broken('node ${node.id} has child $child, not made before it');
      }
    }
    _nodes[node.id] = ValueNotifier(node);
  }

  void _update(int id, BlockNode Function(BlockNode node) change) {
    final slot = _slot(id);
    slot.value = change(slot.value!);
  }

  final _retainedForExit = <int, Set<int>>{};
  final _exitSafetyTimers = <int, Timer>{};

  List<int> _descendants(int id) {
    final result = <int>[];
    void collect(int current) {
      final node = _nodes[current]?.value;
      if (node == null) return;
      for (final childId in node.children) {
        result.add(childId);
        collect(childId);
      }
    }

    collect(id);
    return result;
  }

  void _drop(List<int> ids) {
    for (final id in ids) {
      if (_retainedForExit.values.any((s) => s.contains(id))) {
        continue;
      }
      final node = _nodes[id]?.value;
      if (node != null && node.props['exit'] != null) {
        final allDescendants = {id, ..._descendants(id)};
        _retainedForExit[id] = allDescendants;
        _exitSafetyTimers[id]?.cancel();
        _exitSafetyTimers[id] = Timer(const Duration(milliseconds: 2500), () {
          _exitSafetyTimers.remove(id);
          finishExit(id);
        });
        continue;
      }
      final slot = _slot(id);
      _nodes.remove(id);
      slot.value = null;
    }
  }

  /// Called when an exiting node has completed its exit animation transition.
  void finishExit(int id) {
    _exitSafetyTimers.remove(id)?.cancel();
    final toDrop = _retainedForExit.remove(id);
    if (toDrop != null) {
      for (final dropId in toDrop) {
        final slot = _slot(dropId);
        _nodes.remove(dropId);
        slot.value = null;
      }
    }
  }

  void _effect(BlocksEffect effect) {
    if (_effects.length >= maxWaitingEffects) _effects.removeAt(0);
    _effects.add(effect);
  }
}

/// What's drawn: the main tree's root, then named modals, bottom to top.
@immutable
class BlocksRoots {
  const BlocksRoots({required this.main, this.modals = const []});
  final int main;
  final List<BlocksModalRoot> modals;
}

@immutable
class BlocksModalRoot {
  const BlocksModalRoot({
    required this.id,
    required this.name,
    required this.size,
    this.sheet,
    required this.dismissible,
  });

  final int id;
  final String name;

  /// `small`, `medium`, `large` or `full`.
  final String size;
  final bool? sheet;
  final bool dismissible;
}

/// A quick dialog waiting for its answer ([BlocksController.answerDialog]).
@immutable
class BlocksDialog {
  const BlocksDialog({
    required this.id,
    required this.kind,
    required this.text,
    this.defaultText,
  });

  final int id;

  /// `confirm`, `alert` or `prompt`.
  final String kind;
  final String text;
  final String? defaultText;
}

/// The view stopped: [details] (file, line, trace) only when the viewer may
/// see them.
@immutable
class BlocksError {
  const BlocksError(this.message, {this.details});
  final String message;
  final String? details;
}

/// Something the view asks of the app, once.
@immutable
sealed class BlocksEffect {
  const BlocksEffect();
}

final class CopyEffect extends BlocksEffect {
  const CopyEffect(this.text);
  final String text;

  @override
  bool operator ==(Object other) => other is CopyEffect && other.text == text;

  @override
  int get hashCode => text.hashCode;
}

/// Asked of the viewer first ([BlocksHost.confirmLink]).
final class OpenLinkEffect extends BlocksEffect {
  const OpenLinkEffect(this.url);
  final String url;

  @override
  bool operator ==(Object other) => other is OpenLinkEffect && other.url == url;

  @override
  int get hashCode => url.hashCode;
}

final class OpenChannelEffect extends BlocksEffect {
  const OpenChannelEffect(this.channelId);
  final String channelId;

  @override
  bool operator ==(Object other) =>
      other is OpenChannelEffect && other.channelId == channelId;

  @override
  int get hashCode => channelId.hashCode;
}

/// A sound from the plugin's assets; its end goes back as `sound_end`.
final class SoundEffect extends BlocksEffect {
  const SoundEffect(this.id, this.asset, this.volume);
  final int id;
  final String asset;
  final double volume;

  @override
  bool operator ==(Object other) =>
      other is SoundEffect &&
      other.id == id &&
      other.asset == asset &&
      other.volume == volume;

  @override
  int get hashCode => Object.hash(id, asset, volume);
}

/// A batch the controller can't read. Its message names the op and field,
/// never the batch's content.
class _Broken implements Exception {
  _Broken(this.message);
  final String message;

  @override
  String toString() => 'Broken view batch: $message';
}

(int, List<_Op>) _read(String json) {
  final Object? decoded;
  try {
    decoded = jsonDecode(json);
  } on FormatException {
    throw _Broken('not JSON');
  }
  final batch = _object(decoded, 'the batch');
  final seq = _int(batch['seq'], 'seq');
  final ops = _list(batch['ops'], 'ops');
  return (
    seq,
    [
      for (final (index, op) in ops.indexed)
        _op(_object(op, 'op $index'), index),
    ],
  );
}

_Op _op(Map<String, Object?> op, int index) {
  final kind = op['op'];
  String at(String field) => 'op $index ($kind): $field';
  int id() => _int(op['id'], at('id'));
  return switch (kind) {
    'create' => _Create(
      BlockNode(
        id: id(),
        block: _string(op['block'], at('block')),
        key: op['key'] == null ? null : _string(op['key'], at('key')),
        props: Map.unmodifiable(_object(op['props'], at('props'))),
        events: List.unmodifiable(_strings(op['events'], at('events'))),
        children: List.unmodifiable(_ids(op['children'], at('children'))),
      ),
    ),
    'props' => _Props(
      id(),
      _object(op['set'], at('set')),
      _strings(op['unset'], at('unset')),
    ),
    'events' => _Events(
      id(),
      List.unmodifiable(_strings(op['events'], at('events'))),
    ),
    'children' => _Children(
      id(),
      List.unmodifiable(_ids(op['children'], at('children'))),
    ),
    'drop' => _Drop(_ids(op['ids'], at('ids'))),
    'roots' => _Roots(
      BlocksRoots(
        main: _int(op['main'], at('main')),
        modals: List.unmodifiable([
          for (final (n, modal) in _list(op['modals'], at('modals')).indexed)
            _modal(_object(modal, at('modals[$n]')), at('modals[$n]')),
        ]),
      ),
    ),
    'draw' => _Draw(
      id(),
      [
        for (final (n, item) in _list(op['set'], at('set')).indexed)
          _item(item, at('set[$n]')),
      ],
      _strings(op['remove'], at('remove')),
      op['order'] == null ? null : _strings(op['order'], at('order')),
    ),
    'dialog' => _Dialog(
      BlocksDialog(
        id: id(),
        kind: _choice(op['kind'], at('kind'), const [
          'confirm',
          'alert',
          'prompt',
        ]),
        text: _string(op['text'], at('text')),
        defaultText: op['default'] == null
            ? null
            : _string(op['default'], at('default')),
      ),
    ),
    'frames' => _Frames(_bool(op['on'], at('on'))),
    'back_handler' => _BackHandler(_bool(op['on'], at('on'))),
    'back' => _Back(id(), _bool(op['handled'], at('handled'))),
    'sound' => _Effect(
      SoundEffect(
        id(),
        _string(op['asset'], at('asset')),
        _number(op['volume'], at('volume')),
      ),
    ),
    'copy' => _Effect(CopyEffect(_string(op['text'], at('text')))),
    'open_link' => _Effect(OpenLinkEffect(_string(op['url'], at('url')))),
    'open_channel' => _Effect(
      OpenChannelEffect(_string(op['channel_id'], at('channel_id'))),
    ),
    'error' => _Error(
      BlocksError(
        _string(op['message'], at('message')),
        details: op['details'] == null
            ? null
            : _string(op['details'], at('details')),
      ),
    ),
    'too_old' => const _TooOld(),
    'closed' => _Closed(
      _choice(op['reason'], at('reason'), const [
        'plugin_stopped',
        'lost_access',
        'channel_deleted',
        'unavailable',
        'activity_ended',
        'left_call',
      ]),
    ),
    _ => throw _Broken('op $index: unknown op'),
  };
}

BlocksModalRoot _modal(Map<String, Object?> modal, String at) =>
    BlocksModalRoot(
      id: _int(modal['id'], '$at.id'),
      name: _string(modal['name'], '$at.name'),
      size: _choice(modal['size'], '$at.size', const [
        'small',
        'medium',
        'large',
        'full',
      ]),
      sheet: modal.containsKey('sheet') && modal['sheet'] != null
          ? _bool(modal['sheet'], '$at.sheet')
          : null,
      dismissible: _bool(modal['dismissible'], '$at.dismissible'),
    );

Map<String, Object?> _item(Object? item, String at) {
  final object = _object(item, at);
  _string(object['id'], '$at.id');
  return Map.unmodifiable(object);
}

Map<String, Object?> _object(Object? value, String what) =>
    value is Map<String, Object?>
    ? value
    : throw _Broken('$what must be an object');

List<Object?> _list(Object? value, String what) =>
    value is List<Object?> ? value : throw _Broken('$what must be a list');

int _int(Object? value, String what) => value is int && value >= 0
    ? value
    : throw _Broken('$what must be a whole number');

double _number(Object? value, String what) => value is num && value.isFinite
    ? value.toDouble()
    : throw _Broken('$what must be a number');

String _string(Object? value, String what) =>
    value is String ? value : throw _Broken('$what must be a string');

bool _bool(Object? value, String what) =>
    value is bool ? value : throw _Broken('$what must be true or false');

String _choice(Object? value, String what, List<String> choices) =>
    value is String && choices.contains(value)
    ? value
    : throw _Broken('$what must be one of ${choices.join(', ')}');

List<String> _strings(Object? value, String what) => [
  for (final item in _list(value, what)) _string(item, what),
];

List<int> _ids(Object? value, String what) => [
  for (final item in _list(value, what)) _int(item, what),
];

/// One op, read; applying it may still find the store doesn't match it.
sealed class _Op {
  const _Op();
  void apply(BlocksController controller);
}

final class _Create extends _Op {
  const _Create(this.node);
  final BlockNode node;

  @override
  void apply(BlocksController controller) => controller._create(node);
}

final class _Props extends _Op {
  const _Props(this.id, this.set, this.unset);
  final int id;
  final Map<String, Object?> set;
  final List<String> unset;

  @override
  void apply(BlocksController controller) => controller._update(id, (node) {
    final props = {...node.props, ...set};
    for (final name in unset) {
      props.remove(name);
    }
    return node.copyWith(props: Map.unmodifiable(props));
  });
}

final class _Events extends _Op {
  const _Events(this.id, this.events);
  final int id;
  final List<String> events;

  @override
  void apply(BlocksController controller) =>
      controller._update(id, (node) => node.copyWith(events: events));
}

final class _Children extends _Op {
  const _Children(this.id, this.children);
  final int id;
  final List<int> children;

  @override
  void apply(BlocksController controller) {
    for (final child in children) {
      controller._slot(child);
    }
    controller._update(id, (node) => node.copyWith(children: children));
  }
}

final class _Drop extends _Op {
  const _Drop(this.ids);
  final List<int> ids;

  @override
  void apply(BlocksController controller) => controller._drop(ids);
}

final class _Roots extends _Op {
  const _Roots(this.roots);
  final BlocksRoots roots;

  @override
  void apply(BlocksController controller) => controller._roots = roots;
}

/// Removes `remove`, then puts each item of `set` in place of the item with
/// its id or at the end; `order`, when sent, is then every item id in
/// drawing order (`Patch::Draw`).
final class _Draw extends _Op {
  const _Draw(this.id, this.set, this.remove, this.order);
  final int id;
  final List<Map<String, Object?>> set;
  final List<String> remove;
  final List<String>? order;

  @override
  void apply(BlocksController controller) => controller._update(id, (node) {
    final items = [
      for (final item in node.items)
        if (!remove.contains(item['id'])) item,
    ];
    for (final item in set) {
      final at = items.indexWhere((kept) => kept['id'] == item['id']);
      if (at < 0) {
        items.add(item);
      } else {
        items[at] = item;
      }
    }
    final order = this.order;
    if (order == null) return node.copyWith(items: List.unmodifiable(items));
    final byId = {for (final item in items) item['id']! as String: item};
    if (order.length != byId.length || !order.every(byId.containsKey)) {
      throw _Broken('draw $id: order isn\'t the items');
    }
    return node.copyWith(
      items: List.unmodifiable([for (final id in order) byId[id]!]),
    );
  });
}

final class _Dialog extends _Op {
  const _Dialog(this.dialog);
  final BlocksDialog dialog;

  @override
  void apply(BlocksController controller) => controller._dialogs.add(dialog);
}

final class _Frames extends _Op {
  const _Frames(this.on);
  final bool on;

  @override
  void apply(BlocksController controller) => controller._framesOn = on;
}

final class _BackHandler extends _Op {
  const _BackHandler(this.on);
  final bool on;

  @override
  void apply(BlocksController controller) => controller._backHandler = on;
}

final class _Back extends _Op {
  const _Back(this.id, this.handled);
  final int id;
  final bool handled;

  @override
  void apply(BlocksController controller) =>
      controller._answerBack(id, handled);
}

final class _Effect extends _Op {
  const _Effect(this.effect);
  final BlocksEffect effect;

  @override
  void apply(BlocksController controller) => controller._effect(effect);
}

final class _Error extends _Op {
  const _Error(this.error);
  final BlocksError error;

  @override
  void apply(BlocksController controller) => controller._stop(error);
}

final class _TooOld extends _Op {
  const _TooOld();

  @override
  void apply(BlocksController controller) {
    controller._tooOld = true;
    controller._stop(null);
  }
}

final class _Closed extends _Op {
  const _Closed(this.reason);
  final String reason;

  @override
  void apply(BlocksController controller) {
    controller._closed = reason;
    controller._stop(null);
  }
}
