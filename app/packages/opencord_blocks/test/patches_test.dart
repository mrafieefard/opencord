// Replays the tree diff's fixtures (crates/opencord-plugins/tests/fixtures/
// patches), the contract between the core and this renderer: each fixture's
// `before` tree is created the way the core numbers a first render
// (pre-order ids from 1), its `ops` are applied as one batch, and the node
// store must then hold exactly its `after` tree.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

const _folder = '../../../crates/opencord-plugins/tests/fixtures/patches';

/// The fixtures the Rust side requires (tests/patches.rs); the folder may
/// hold more, and every one is replayed.
const _required = [
  'first_render',
  'append',
  'insert_middle_keyed',
  'move_keyed',
  'remove_with_subtree',
  'prop_change',
  'prop_unset',
  'same_key_new_block',
  'unkeyed_by_position',
  'events_change',
  'canvas_items',
  'identical',
];

List<File> _fixtures() {
  final files =
      Directory(_folder)
          .listSync()
          .whereType<File>()
          .where((file) => file.path.endsWith('.json'))
          .toList()
        ..sort((a, b) => a.path.compareTo(b.path));
  return files;
}

String _name(File file) =>
    file.uri.pathSegments.last.replaceFirst(RegExp(r'\.json$'), '');

/// The ops a first render of [tree] gives: ids in pre-order from 1,
/// children created before their parents, a canvas's items in a `draw` op.
List<Map<String, Object?>> _firstRender(Map<String, Object?> tree) {
  var next = 0;
  final creates = <Map<String, Object?>>[];
  final draws = <Map<String, Object?>>[];
  int walk(Map<String, Object?> node) {
    final id = ++next;
    final props = Map<String, Object?>.of(
      node['props']! as Map<String, Object?>,
    );
    final items = node['block'] == 'canvas' ? props.remove('items') : null;
    final children = [
      for (final child in node['children']! as List<Object?>)
        walk(child! as Map<String, Object?>),
    ];
    creates.add(
      create(
        id,
        node['block']! as String,
        key: node['key'] as String?,
        props: props,
        events: [
          for (final e in node['events']! as List<Object?>) e! as String,
        ],
        children: children,
      ),
    );
    if (items != null && (items as List<Object?>).isNotEmpty) {
      draws.add({
        'op': 'draw',
        'id': id,
        'set': items,
        'remove': const <String>[],
      });
    }
    return id;
  }

  walk(tree);
  return [...creates, ...draws];
}

/// A fixture tree in the shape [_held] gives: a canvas's items apart from
/// its props, events sorted.
Map<String, Object?> _canonical(Map<String, Object?> tree) {
  final props = Map<String, Object?>.of(tree['props']! as Map<String, Object?>);
  final items = tree['block'] == 'canvas'
      ? (props.remove('items') ?? const <Object?>[])
      : const <Object?>[];
  return {
    'block': tree['block'],
    'key': tree['key'],
    'props': props,
    'items': items,
    'events': [for (final e in tree['events']! as List<Object?>) e]
      ..sort((a, b) => (a! as String).compareTo(b! as String)),
    'children': [
      for (final child in tree['children']! as List<Object?>)
        _canonical(child! as Map<String, Object?>),
    ],
  };
}

/// The tree the store holds, from the one node no other node holds. Fails
/// if the store holds more than one tree, or nodes nothing reaches.
Map<String, Object?> _storeTree(BlocksController controller) {
  final nodes = {for (final node in controller.nodes) node.id: node};
  final held = {for (final node in nodes.values) ...node.children};
  final tops = nodes.keys.where((id) => !held.contains(id)).toList();
  expect(tops, hasLength(1), reason: 'one root');
  var reached = 0;
  Map<String, Object?> build(int id) {
    reached++;
    final node = nodes[id]!;
    return {
      'block': node.block,
      'key': node.key,
      'props': node.props,
      'items': node.items,
      'events': [...node.events]..sort(),
      'children': [for (final child in node.children) build(child)],
    };
  }

  final tree = build(tops.single);
  expect(reached, nodes.length, reason: 'every node is in the tree');
  return tree;
}

void main() {
  final fixtures = _fixtures();

  // Fails if the folder moved (every replay below would then be skipped
  // silently) or a fixture the Rust side requires is gone.
  test('every_required_fixture_is_there', () {
    final names = fixtures.map(_name).toSet();
    for (final name in _required) {
      expect(names, contains(name));
    }
  });

  for (final file in fixtures) {
    // Fails if the controller applies any op differently from the core's
    // reference renderer (tests/patches.rs): a child list not replaced
    // whole, an unset prop kept, a drop that leaves nodes, canvas items not
    // put in place by id or not reordered by `order`.
    test('the_${_name(file)}_fixture_replays', () {
      final fixture =
          jsonDecode(file.readAsStringSync()) as Map<String, Object?>;
      final controller = BlocksController();
      addTearDown(controller.dispose);
      var seq = 0;
      final before = fixture['before'] as Map<String, Object?>?;
      if (before != null) {
        controller.applyBatch(batch(++seq, _firstRender(before)));
        expect(_storeTree(controller), _canonical(before));
      }
      final ops = [
        for (final op in fixture['ops']! as List<Object?>)
          op! as Map<String, Object?>,
      ];
      controller.applyBatch(batch(++seq, ops));
      expect(controller.error, isNull);
      expect(
        _storeTree(controller),
        _canonical(fixture['after']! as Map<String, Object?>),
      );
    });
  }
}
