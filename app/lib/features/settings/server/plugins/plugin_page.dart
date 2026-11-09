import 'dart:async';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/logs/app_log.dart';
import 'package:opencord/core/providers/plugins_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/plugins/dev_tokens.dart';
import 'package:opencord/features/settings/server/plugins/plugin_config_fields.dart';
import 'package:opencord/features/settings/server/plugins/review_dialog.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

enum PluginPageTab { overview, permissions, settings, log, devTokens }

const _pluginZipTypes = XTypeGroup(label: 'Plugins', extensions: ['zip']);

/// A plugin's detail page (§17.2).
class PluginPage extends ConsumerStatefulWidget {
  const PluginPage({
    super.key,
    required this.serverKey,
    required this.pluginId,
    this.initialTab = PluginPageTab.overview,
    this.onBack,
  });

  final String serverKey;
  final String pluginId;
  final PluginPageTab initialTab;
  final VoidCallback? onBack;

  @override
  ConsumerState<PluginPage> createState() => _PluginPageState();
}

class _PluginPageState extends ConsumerState<PluginPage> {
  late PluginPageTab _tab = widget.initialTab;
  final SettingsChanges _localChanges = SettingsChanges();

  @override
  void dispose() {
    _localChanges.dispose();
    super.dispose();
  }

  Future<void> _restart() async {
    try {
      await ref
          .read(repositoryProvider)
          .restartPlugin(widget.serverKey, widget.pluginId);
      if (mounted) showOcToast(context, 'Plugin restarted');
    } on RepoException catch (e) {
      if (mounted) showOcToast(context, e.message);
    }
  }

  Future<void> _update() async {
    final file = await openFile(acceptedTypeGroups: const [_pluginZipTypes]);
    if (file == null || !mounted) return;
    final bytes = await file.readAsBytes();
    if (!mounted) return;
    await showReviewDialog(
      context,
      serverKey: widget.serverKey,
      uploadBytes: bytes,
      fileName: file.name,
    );
  }

  Future<void> _uninstall(Plugin plugin) async {
    var deleteData = false;
    final confirmed = await showOcDialog<bool>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, setDialogState) => OcDialog(
          title: 'Uninstall ${plugin.name}?',
          width: 440,
          actions: [
            OcButton.ghost(
              label: 'Cancel',
              onPressed: () => Navigator.of(context).pop(false),
            ),
            OcButton.primary(
              label: 'Uninstall',
              onPressed: () => Navigator.of(context).pop(true),
            ),
          ],
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                'Are you sure you want to uninstall ${plugin.name}?',
                style: OcText.body.copyWith(color: context.oc.textSecondary),
              ),
              const SizedBox(height: OcSpace.s16),
              Hoverable(
                onTap: () => setDialogState(() => deleteData = !deleteData),
                builder: (context, state) => Row(
                  children: [
                    Icon(
                      deleteData
                          ? OcIcons.checkBox
                          : OcIcons.checkBoxOutlineBlank,
                      size: 18,
                      color: deleteData
                          ? context.oc.text
                          : context.oc.textMuted,
                    ),
                    const SizedBox(width: OcSpace.s8),
                    Expanded(
                      child: Text(
                        'Also delete its data and files',
                        style: OcText.body.copyWith(color: context.oc.text),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );

    if (confirmed == true && mounted) {
      try {
        await ref
            .read(repositoryProvider)
            .uninstallPlugin(
              widget.serverKey,
              plugin.id,
              deleteData: deleteData,
            );
        if (mounted) {
          showOcToast(context, '${plugin.name} uninstalled');
          widget.onBack?.call();
        }
      } on RepoException catch (e) {
        if (mounted) showOcToast(context, e.message);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final pluginsAsync = ref.watch(pluginsProvider(widget.serverKey));
    final plugins = pluginsAsync.value ?? const [];
    final plugin = plugins.where((p) => p.id == widget.pluginId).firstOrNull;

    if (pluginsAsync.isLoading && plugin == null) {
      return const Center(
        child: SizedBox(
          width: 24,
          height: 24,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
      );
    }

    if (plugin == null) {
      return Center(
        child: Text(
          'Plugin not found.',
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      );
    }

    final parentChanges = SettingsScope.changesOf(context);
    final changes = parentChanges ?? _localChanges;

    final content = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // Crashed banner
        if (plugin.state == PluginState.crashed) ...[
          Container(
            padding: const EdgeInsets.symmetric(
              horizontal: OcSpace.s16,
              vertical: OcSpace.s12,
            ),
            margin: const EdgeInsets.only(bottom: OcSpace.s16),
            decoration: BoxDecoration(
              color: colors.hover,
              borderRadius: BorderRadius.circular(OcRadius.section),
              border: Border.all(color: colors.border),
            ),
            child: Row(
              children: [
                Icon(OcIcons.error, size: 20, color: colors.text),
                const SizedBox(width: OcSpace.s12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        'Plugin has crashed',
                        style: OcText.body.copyWith(
                          color: colors.text,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      if (plugin.statusReason.isNotEmpty) ...[
                        const SizedBox(height: 2),
                        Text(
                          plugin.statusReason,
                          style: OcText.small.copyWith(
                            color: colors.textSecondary,
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
                const SizedBox(width: OcSpace.s12),
                OcButton(
                  label: 'Start again',
                  dense: true,
                  onPressed: _restart,
                ),
              ],
            ),
          ),
        ],

        // Header with icon, title and badges
        Row(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            if (widget.onBack != null) ...[
              OcIconButton(
                icon: OcIcons.arrowBack,
                tooltip: 'Back to plugins',
                onPressed: widget.onBack!,
              ),
              const SizedBox(width: OcSpace.s8),
            ],
            Container(
              width: 44,
              height: 44,
              decoration: BoxDecoration(
                color: colors.hover,
                borderRadius: BorderRadius.circular(10),
                border: Border.all(color: colors.border),
              ),
              alignment: Alignment.center,
              child: Icon(
                OcIcons.extension,
                size: 24,
                color: colors.textSecondary,
              ),
            ),
            const SizedBox(width: OcSpace.s12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Row(
                    children: [
                      Flexible(
                        child: Text(
                          plugin.name,
                          style: OcText.title.copyWith(color: colors.text),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                      const SizedBox(width: OcSpace.s8),
                      Text(
                        plugin.version,
                        style: OcText.body.copyWith(color: colors.textMuted),
                      ),
                    ],
                  ),
                  const SizedBox(height: 2),
                  Text(
                    plugin.author,
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                ],
              ),
            ),
            PluginSignatureBadge(
              signature: plugin.signature,
              githubCheck: plugin.githubCheck,
            ),
            const SizedBox(width: 6),
            PluginStatusBadge(state: plugin.state),
          ],
        ),
        const SizedBox(height: OcSpace.s20),

        // Tabs
        Align(
          alignment: Alignment.centerLeft,
          child: ChoiceChips<PluginPageTab>(
            options: const [
              (PluginPageTab.overview, 'Overview'),
              (PluginPageTab.permissions, 'Permissions'),
              (PluginPageTab.settings, 'Settings'),
              (PluginPageTab.log, 'Log'),
              (PluginPageTab.devTokens, 'Dev tokens'),
            ],
            value: _tab,
            onChanged: (tab) => setState(() => _tab = tab),
          ),
        ),
        const SizedBox(height: OcSpace.s20),

        // Tab contents
        Visibility(
          visible: _tab == PluginPageTab.overview,
          maintainState: true,
          child: _OverviewTab(
            plugin: plugin,
            onRestart: _restart,
            onUpdate: _update,
            onUninstall: () => _uninstall(plugin),
          ),
        ),
        Visibility(
          visible: _tab == PluginPageTab.permissions,
          maintainState: true,
          child: _PermissionsTab(serverKey: widget.serverKey, plugin: plugin),
        ),
        Visibility(
          visible: _tab == PluginPageTab.settings,
          maintainState: true,
          child: _SettingsTab(serverKey: widget.serverKey, plugin: plugin),
        ),
        Visibility(
          visible: _tab == PluginPageTab.log,
          maintainState: true,
          child: _LogTab(
            serverKey: widget.serverKey,
            pluginId: widget.pluginId,
          ),
        ),
        Visibility(
          visible: _tab == PluginPageTab.devTokens,
          maintainState: true,
          child: DevTokensSection(
            serverKey: widget.serverKey,
            pluginId: widget.pluginId,
          ),
        ),
      ],
    );

    // The settings dialog scrolls, pads and shows the save bar itself, and
    // gives its pages an unbounded height: a Stack can't size itself in it.
    if (parentChanges != null) return content;

    return SettingsScope(
      changes: changes,
      child: Stack(
        children: [
          Positioned.fill(
            child: SingleChildScrollView(
              padding: const EdgeInsets.fromLTRB(
                OcSpace.s24,
                OcSpace.s20,
                OcSpace.s24,
                OcSpace.s24 + SaveBar.height + 16,
              ),
              child: content,
            ),
          ),
          Positioned(
            left: OcSpace.s16,
            right: OcSpace.s16,
            bottom: OcSpace.s16,
            child: SaveBar(changes: changes),
          ),
        ],
      ),
    );
  }
}

class _OverviewTab extends StatelessWidget {
  const _OverviewTab({
    required this.plugin,
    required this.onRestart,
    required this.onUpdate,
    required this.onUninstall,
  });

  final Plugin plugin;
  final VoidCallback onRestart;
  final VoidCallback onUpdate;
  final VoidCallback onUninstall;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final dbMb = (plugin.databaseBytes / (1024 * 1024)).toStringAsFixed(1);
    final filesMb = (plugin.filesBytes / (1024 * 1024)).toStringAsFixed(1);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (plugin.description.isNotEmpty) ...[
          Text(
            plugin.description,
            style: OcText.body.copyWith(color: colors.text),
          ),
          const SizedBox(height: OcSpace.s16),
        ],
        SettingsSection(
          title: 'Details',
          children: [
            if (plugin.github.isNotEmpty)
              SettingsRow(
                title: 'GitHub',
                subtitle: plugin.github,
                trailing: plugin.githubCheck == GithubCheck.verified
                    ? Icon(OcIcons.check, size: 16, color: colors.text)
                    : null,
              ),
            if (plugin.license.isNotEmpty)
              SettingsRow(title: 'Licence', subtitle: plugin.license),
            if (plugin.keyFingerprint.isNotEmpty)
              SettingsRow(
                title: 'Key fingerprint',
                subtitle: plugin.keyFingerprint,
                mono: true,
              ),
            SettingsRow(
              title: 'Installed',
              subtitle: longDate(plugin.installedAt),
            ),
            SettingsRow(title: 'Updated', subtitle: longDate(plugin.updatedAt)),
          ],
        ),
        const SizedBox(height: OcSpace.s16),
        SettingsSection(
          title: 'Storage usage',
          children: [
            SettingsRow(title: 'Database', subtitle: '$dbMb MB'),
            SettingsRow(title: 'Files', subtitle: '$filesMb MB'),
          ],
        ),
        const SizedBox(height: OcSpace.s20),
        const SectionLabel('Actions'),
        const SizedBox(height: OcSpace.s8),
        Wrap(
          spacing: OcSpace.s12,
          runSpacing: OcSpace.s12,
          children: [
            OcButton(
              label: 'Restart',
              icon: OcIcons.refresh,
              onPressed: onRestart,
            ),
            OcButton(
              label: 'Update',
              icon: OcIcons.upload,
              onPressed: onUpdate,
            ),
            OcButton(
              label: 'Uninstall',
              icon: OcIcons.delete,
              onPressed: onUninstall,
            ),
          ],
        ),
      ],
    );
  }
}

class _PermissionsTab extends ConsumerStatefulWidget {
  const _PermissionsTab({required this.serverKey, required this.plugin});

  final String serverKey;
  final Plugin plugin;

  @override
  ConsumerState<_PermissionsTab> createState() => _PermissionsTabState();
}

class _PermissionsTabState extends ConsumerState<_PermissionsTab>
    with SaveBarChanges {
  late List<String> _saved = widget.plugin.permissions
      .where((p) => p.granted)
      .map((p) => p.name)
      .toList();
  late List<String> _draft = List.of(_saved);

  @override
  void didUpdateWidget(_PermissionsTab oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.plugin != widget.plugin && !unsavedChanges) {
      _saved = widget.plugin.permissions
          .where((p) => p.granted)
          .map((p) => p.name)
          .toList();
      _draft = List.of(_saved);
    }
  }

  @override
  bool get unsavedChanges =>
      _draft.length != _saved.length ||
      !_draft.every((item) => _saved.contains(item));

  @override
  Future<void> saveChanges() async {
    final draft = _draft;
    await ref
        .read(repositoryProvider)
        .setPluginPermissions(widget.serverKey, widget.plugin.id, draft);
    _saved = draft;
  }

  @override
  void discardChanges() {
    if (mounted) setState(() => _draft = List.of(_saved));
  }

  void _toggle(String name, bool on) {
    setState(() {
      if (on) {
        if (!_draft.contains(name)) _draft.add(name);
      } else {
        _draft.remove(name);
      }
    });
    changesEdited();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (widget.plugin.permissions.isEmpty)
          Container(
            padding: const EdgeInsets.all(OcSpace.s24),
            decoration: BoxDecoration(
              color: colors.surface,
              borderRadius: BorderRadius.circular(OcRadius.section),
              border: Border.all(color: colors.border),
            ),
            alignment: Alignment.center,
            child: Text(
              'No permissions requested.',
              style: OcText.body.copyWith(color: colors.textMuted),
            ),
          )
        else
          SettingsSection(
            title: 'Permissions',
            children: [
              for (final p in widget.plugin.permissions)
                Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: OcSpace.s16,
                    vertical: OcSpace.s12,
                  ),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          p.name,
                          style: OcText.body.copyWith(color: colors.text),
                        ),
                      ),
                      OcSwitch(
                        value: _draft.contains(p.name),
                        onChanged: (on) => _toggle(p.name, on),
                      ),
                    ],
                  ),
                ),
            ],
          ),
        if (widget.plugin.domains.isNotEmpty) ...[
          const SizedBox(height: OcSpace.s16),
          SettingsSection(
            title: 'Network domains',
            children: [
              for (final domain in widget.plugin.domains)
                SettingsRow(title: domain, icon: OcIcons.dns, mono: true),
            ],
          ),
        ],
      ],
    );
  }
}

class _SettingsTab extends ConsumerStatefulWidget {
  const _SettingsTab({required this.serverKey, required this.plugin});

  final String serverKey;
  final Plugin plugin;

  @override
  ConsumerState<_SettingsTab> createState() => _SettingsTabState();
}

class _SettingsTabState extends ConsumerState<_SettingsTab>
    with SaveBarChanges {
  final Map<String, String> _values = {};
  late Map<String, String> _saved = {
    for (final val in widget.plugin.config) val.key: val.value,
  };
  bool _dirty = false;

  String _initialValueFor(PluginConfigField field) {
    if (field.kind == PluginConfigKind.secret) {
      return '';
    }
    final saved = _saved[field.key];
    if (field.kind == PluginConfigKind.choice &&
        (saved == null || saved.isEmpty) &&
        field.choices.isNotEmpty) {
      return field.choices.first;
    }
    return saved ?? '';
  }

  @override
  void initState() {
    super.initState();
    for (final field in widget.plugin.configFields) {
      _values[field.key] = _initialValueFor(field);
    }
  }

  @override
  void didUpdateWidget(_SettingsTab oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.plugin != widget.plugin && !unsavedChanges) {
      _saved = {for (final val in widget.plugin.config) val.key: val.value};
      for (final field in widget.plugin.configFields) {
        _values[field.key] = _initialValueFor(field);
      }
    }
  }

  @override
  bool get unsavedChanges => _dirty;

  @override
  Future<void> saveChanges() async {
    final configToSend = <PluginConfigValue>[
      for (final field in widget.plugin.configFields)
        if (field.kind == PluginConfigKind.secret)
          PluginConfigValue(
            key: field.key,
            // A kept secret is sent empty (§17.2, review focus)
            value: _values[field.key]?.trim() ?? '',
            hasValue: (_values[field.key]?.trim() ?? '').isNotEmpty,
          )
        else
          PluginConfigValue(
            key: field.key,
            value: _values[field.key] ?? '',
            hasValue: (_values[field.key] ?? '').isNotEmpty,
          ),
    ];

    await ref
        .read(repositoryProvider)
        .setPluginConfig(widget.serverKey, widget.plugin.id, configToSend);

    _saved = {for (final v in configToSend) v.key: v.value};
    _dirty = false;
  }

  @override
  void discardChanges() {
    if (!mounted) return;
    setState(() {
      for (final field in widget.plugin.configFields) {
        _values[field.key] = _initialValueFor(field);
      }
      _dirty = false;
    });
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;

    if (widget.plugin.configFields.isEmpty) {
      return Container(
        padding: const EdgeInsets.all(OcSpace.s24),
        decoration: BoxDecoration(
          color: colors.surface,
          borderRadius: BorderRadius.circular(OcRadius.section),
          border: Border.all(color: colors.border),
        ),
        alignment: Alignment.center,
        child: Text(
          'No configuration settings available.',
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      );
    }

    return PluginConfigFields(
      serverKey: widget.serverKey,
      fields: widget.plugin.configFields,
      values: _values,
      secretHint: 'Saved. Leave empty to keep it.',
      onChanged: (key, val) {
        setState(() {
          _values[key] = val;
          _dirty = true;
        });
        changesEdited();
      },
    );
  }
}

enum _LogFilter { all, errors, actions }

class _LogTab extends ConsumerStatefulWidget {
  const _LogTab({required this.serverKey, required this.pluginId});

  final String serverKey;
  final String pluginId;

  @override
  ConsumerState<_LogTab> createState() => _LogTabState();
}

class _LogTabState extends ConsumerState<_LogTab> {
  _LogFilter _filter = _LogFilter.all;
  final List<PluginLogLine> _lines = [];
  final List<PluginLogLine> _buffered = [];
  bool _initialLoaded = false;
  String? _error;
  StreamSubscription? _sub;

  late final OpencordRepository _repo;

  @override
  void initState() {
    super.initState();
    _repo = ref.read(repositoryProvider);
    _initLogStream();
  }

  @override
  void dispose() {
    _sub?.cancel();
    unawaited(
      _repo
          .watchPluginLog(widget.serverKey, widget.pluginId, watch: false)
          .catchError((e) {
            appLog('Failed to unwatch plugin log: $e', name: 'plugins');
          }),
    );
    super.dispose();
  }

  Future<void> _initLogStream() async {
    // 1. Subscribe and buffer live events first (Finding 8)
    try {
      _sub = _repo.events.listen((event) {
        if (event is PluginLogAppended &&
            event.serverKey == widget.serverKey &&
            event.pluginId == widget.pluginId) {
          if (!_initialLoaded) {
            _buffered.addAll(event.lines);
          } else {
            _append(event.lines);
          }
        }
      });
      await _repo.watchPluginLog(
        widget.serverKey,
        widget.pluginId,
        watch: true,
      );
    } on RepoException catch (e) {
      appLog('Failed to watch plugin log: $e', name: 'plugins');
      _showLoadError(e);
      return;
    }

    // 2. Fetch initial log lines
    List<PluginLogLine> initial = const [];
    try {
      initial = await _repo.fetchPluginLog(
        widget.serverKey,
        widget.pluginId,
        limit: 200,
      );
    } on RepoException catch (e) {
      appLog('Failed to fetch initial plugin log: $e', name: 'plugins');
      _showLoadError(e);
      return;
    }

    if (!mounted) return;

    // 3. Merge initial and buffered lines deduplicating by seq
    final Map<int, PluginLogLine> bySeq = {};
    for (final line in initial) {
      bySeq[line.seq] = line;
    }
    for (final line in _buffered) {
      bySeq[line.seq] = line;
    }
    _buffered.clear();
    _initialLoaded = true;

    final sorted = bySeq.values.toList()
      ..sort((a, b) => a.seq.compareTo(b.seq));
    setState(() {
      _lines.clear();
      _lines.addAll(sorted);
    });
  }

  /// The error replaces the list, so what arrives from now on has nowhere to
  /// go: stop listening rather than buffer it until the tab closes.
  void _showLoadError(RepoException error) {
    unawaited(_sub?.cancel());
    _buffered.clear();
    if (mounted) {
      setState(() => _error = 'Failed to load log: ${error.message}');
    }
  }

  void _append(List<PluginLogLine> lines) {
    if (!mounted || lines.isEmpty) return;
    setState(() {
      _lines.addAll(lines);
      // Maximum 5,000 lines kept in the view (§17.2).
      if (_lines.length > 5000) {
        _lines.removeRange(0, _lines.length - 5000);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;

    final filtered = _lines.where((line) {
      return switch (_filter) {
        _LogFilter.all => true,
        _LogFilter.errors => line.level == PluginLogLevel.error,
        _LogFilter.actions => line.level == PluginLogLevel.action,
      };
    }).toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: ChoiceChips<_LogFilter>(
            options: const [
              (_LogFilter.all, 'All'),
              (_LogFilter.errors, 'Errors'),
              (_LogFilter.actions, 'Actions'),
            ],
            value: _filter,
            onChanged: (f) => setState(() => _filter = f),
          ),
        ),
        const SizedBox(height: OcSpace.s12),
        Container(
          height: 380,
          padding: const EdgeInsets.all(OcSpace.s12),
          decoration: BoxDecoration(
            color: colors.surface,
            borderRadius: BorderRadius.circular(OcRadius.section),
            border: Border.all(color: colors.border),
          ),
          child: _error != null
              ? Center(child: InlineError(_error!))
              : filtered.isEmpty
              ? Center(
                  child: Text(
                    'No log entries yet.',
                    style: OcText.mono.copyWith(color: colors.textMuted),
                  ),
                )
              : ListView.builder(
                  itemCount: filtered.length,
                  itemExtent: 26,
                  itemBuilder: (context, i) {
                    final line = filtered[i];
                    return Row(
                      crossAxisAlignment: CrossAxisAlignment.center,
                      children: [
                        Text(
                          clockTime(line.at),
                          style: OcText.mono.copyWith(
                            color: colors.textMuted,
                            fontSize: 11.5,
                          ),
                        ),
                        const SizedBox(width: OcSpace.s8),
                        Text(
                          line.level.name.toUpperCase(),
                          style: OcText.mono.copyWith(
                            color: line.level == PluginLogLevel.error
                                ? colors.text
                                : colors.textSecondary,
                            fontWeight: FontWeight.w600,
                            fontSize: 11,
                          ),
                        ),
                        const SizedBox(width: OcSpace.s8),
                        Expanded(
                          child: Text(
                            line.text,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: OcText.mono.copyWith(
                              color: colors.text,
                              fontSize: 12,
                            ),
                          ),
                        ),
                      ],
                    );
                  },
                ),
        ),
      ],
    );
  }
}
