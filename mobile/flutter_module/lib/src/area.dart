import 'dart:async';

import 'package:flutter/material.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'channel.dart';
import 'gates.dart';
import 'phone_host.dart';
import 'status.dart';

/// One plugin area: the renderer fed by [channel]'s batches, its inputs and
/// status sent back. Draws nothing until the setup arrives.
class PluginAreaApp extends StatefulWidget {
  const PluginAreaApp({super.key, required this.channel});

  final AreaChannel channel;

  @override
  State<PluginAreaApp> createState() => _PluginAreaAppState();
}

class _PluginAreaAppState extends State<PluginAreaApp> {
  final _controller = BlocksController();
  PhoneBlocksHost? _host;
  StreamSubscription<String>? _batches;
  StreamSubscription<String>? _inputs;
  var _statusSent = false;

  @override
  void initState() {
    super.initState();
    _inputs = _controller.inputs.listen(widget.channel.sendInput);
    _controller.addListener(_sendStatus);
    widget.channel.setup.addListener(_setupChanged);
  }

  void _setupChanged() {
    if (widget.channel.setup.value == null) return;
    // Batches are applied from the first setup on: one before it would
    // build widgets with no theme.
    _batches ??= widget.channel.batches.listen((json) {
      _controller.applyBatch(json);
      Gates.batchApplied();
    });
    Gates.startFrameCount();
    if (!_statusSent) _sendStatus();
    setState(() {});
  }

  void _sendStatus() {
    _statusSent = true;
    widget.channel.sendStatus(statusOf(_controller));
  }

  @override
  void dispose() {
    widget.channel.setup.removeListener(_setupChanged);
    _controller.removeListener(_sendStatus);
    unawaited(_batches?.cancel());
    unawaited(_inputs?.cancel());
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final setup = widget.channel.setup.value;
    if (setup == null) return const SizedBox.shrink();
    final theme = blocksThemeOf(setup);
    // One host per area, so its caches last; a new setup only re-themes it.
    final host = (_host ??= PhoneBlocksHost(channel: widget.channel, theme: theme))..theme = theme;
    return MaterialApp(
      debugShowCheckedModeBanner: false,
      color: const Color(0x00000000),
      theme: ThemeData(
        brightness: setup.dark ? Brightness.dark : Brightness.light,
        scaffoldBackgroundColor: const Color(0x00000000),
      ),
      // Over the navigator, so the renderer's text fields and overlays get
      // the setup's scale and insets too.
      builder: (context, navigator) => MediaQuery(
        data: MediaQuery.of(context).copyWith(
          textScaler: TextScaler.linear(setup.textScale),
          padding: setup.safeArea,
          disableAnimations: setup.reduceMotion,
        ),
        child: navigator!,
      ),
      home: Material(
        type: MaterialType.transparency,
        child: BlocksView(controller: _controller, host: host, theme: theme),
      ),
    );
  }
}

/// The renderer's theme from the setup's colours, as the desktop's
/// `buildBlocksTheme` builds it: Inter 14, radius 8. A colour the setup
/// lacks takes the desktop's dark value.
BlocksTheme blocksThemeOf(AreaSetup setup) {
  final colors = {..._fallback, ...setup.colors};
  return BlocksTheme(
    brightness: setup.dark ? Brightness.dark : Brightness.light,
    colors: {for (final name in blockColorNames) name: colors[name]!},
    text: TextStyle(
      fontFamily: 'Inter',
      fontSize: 14,
      height: 20 / 14,
      color: colors['text'],
    ),
    radius: 8,
  );
}

const _fallback = {
  'text': Color(0xFFEDEDEF),
  'text_secondary': Color(0xFFA1A1A8),
  'text_muted': Color(0xFF6E6E76),
  'background': Color(0xFF0E0E10),
  'surface': Color(0xFF18181B),
  'elevated': Color(0xFF1F1F23),
  'border': Color(0xFF222226),
  'accent': Color(0xFFEDEDEF),
  'on_accent': Color(0xFF0E0E10),
  'selected': Color(0xFF27272C),
  'hover': Color(0xFF1C1C20),
  'danger': Color(0xFFF2555A),
};
