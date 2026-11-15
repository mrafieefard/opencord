import 'package:flutter/foundation.dart';

/// One node of a view's tree, as the core's patches describe it. A new
/// object replaces it on every change, so a widget can tell "changed" by
/// identity.
@immutable
class BlockNode {
  const BlockNode({
    required this.id,
    required this.block,
    this.key,
    this.props = const {},
    this.events = const [],
    this.children = const [],
    this.items = const [],
  });

  final int id;
  final String block;
  final String? key;

  /// The props the core checked (Task 1's schema), as plain JSON values.
  final Map<String, Object?> props;

  /// The events the plugin handles, named without `on_`.
  final List<String> events;

  /// Child node ids, in order.
  final List<int> children;

  /// A canvas's drawing items, in drawing order (`draw` ops).
  final List<Map<String, Object?>> items;

  bool handles(String event) => events.contains(event);

  BlockNode copyWith({
    Map<String, Object?>? props,
    List<String>? events,
    List<int>? children,
    List<Map<String, Object?>>? items,
  }) => BlockNode(
    id: id,
    block: block,
    key: key,
    props: props ?? this.props,
    events: events ?? this.events,
    children: children ?? this.children,
    items: items ?? this.items,
  );
}
