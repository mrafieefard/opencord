import 'dart:collection';

import 'package:flutter/widgets.dart';
import 'package:re_highlight/languages/bash.dart';
import 'package:re_highlight/languages/c.dart';
import 'package:re_highlight/languages/cpp.dart';
import 'package:re_highlight/languages/csharp.dart';
import 'package:re_highlight/languages/css.dart';
import 'package:re_highlight/languages/dart.dart';
import 'package:re_highlight/languages/diff.dart';
import 'package:re_highlight/languages/go.dart';
import 'package:re_highlight/languages/ini.dart';
import 'package:re_highlight/languages/java.dart';
import 'package:re_highlight/languages/javascript.dart';
import 'package:re_highlight/languages/json.dart';
import 'package:re_highlight/languages/kotlin.dart';
import 'package:re_highlight/languages/lua.dart';
import 'package:re_highlight/languages/markdown.dart';
import 'package:re_highlight/languages/php.dart';
import 'package:re_highlight/languages/python.dart';
import 'package:re_highlight/languages/ruby.dart';
import 'package:re_highlight/languages/rust.dart';
import 'package:re_highlight/languages/shell.dart';
import 'package:re_highlight/languages/sql.dart';
import 'package:re_highlight/languages/swift.dart';
import 'package:re_highlight/languages/typescript.dart';
import 'package:re_highlight/languages/xml.dart';
import 'package:re_highlight/languages/yaml.dart';
import 'package:re_highlight/re_highlight.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// Cache key for highlighted spans.
@immutable
class _CacheKey {
  const _CacheKey(this.code, this.language, this.colors);

  final String code;
  final String language;
  final OcColors colors;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is _CacheKey &&
          runtimeType == other.runtimeType &&
          code == other.code &&
          language == other.language &&
          colors == other.colors;

  @override
  int get hashCode => Object.hash(code, language, colors);
}

final _cache = <_CacheKey, InlineSpan>{};

Highlight? _highlightInstance;

Highlight _getHighlight() {
  if (_highlightInstance != null) return _highlightInstance!;
  final hl = Highlight();

  hl.registerLanguage('python', langPython);
  hl.registerLanguage('javascript', langJavascript);
  hl.registerLanguage('typescript', langTypescript);
  hl.registerLanguage('rust', langRust);
  hl.registerLanguage('dart', langDart);
  hl.registerLanguage('go', langGo);
  hl.registerLanguage('java', langJava);
  hl.registerLanguage('kotlin', langKotlin);
  hl.registerLanguage('swift', langSwift);
  hl.registerLanguage('lua', langLua);
  hl.registerLanguage('php', langPhp);
  hl.registerLanguage('ruby', langRuby);
  hl.registerLanguage('sql', langSql);
  hl.registerLanguage('css', langCss);
  hl.registerLanguage('json', langJson);
  hl.registerLanguage('yaml', langYaml);
  hl.registerLanguage('diff', langDiff);
  hl.registerLanguage('markdown', langMarkdown);
  hl.registerLanguage('c', langC);
  hl.registerLanguage('cpp', langCpp);
  hl.registerLanguage('csharp', langCsharp);
  hl.registerLanguage('bash', langBash);
  hl.registerLanguage('shell', langShell);
  hl.registerLanguage('xml', langXml);
  hl.registerLanguage('ini', langIni);

  hl.registerAliases(['py'], 'python');
  hl.registerAliases(['js', 'jsx'], 'javascript');
  hl.registerAliases(['ts', 'tsx'], 'typescript');
  hl.registerAliases(['rs'], 'rust');
  hl.registerAliases(['golang'], 'go');
  hl.registerAliases(['kt'], 'kotlin');
  hl.registerAliases(['rb'], 'ruby');
  hl.registerAliases(['yml'], 'yaml');
  hl.registerAliases(['md'], 'markdown');
  hl.registerAliases(['h'], 'c');
  hl.registerAliases(['c++'], 'cpp');
  hl.registerAliases(['cs'], 'csharp');
  hl.registerAliases(['sh', 'zsh'], 'bash');
  hl.registerAliases(['html'], 'xml');
  hl.registerAliases(['toml'], 'ini');

  _highlightInstance = hl;
  return hl;
}

class _PrefixThemeMap extends MapBase<String, TextStyle> {
  _PrefixThemeMap(this._entries);

  final Map<String, TextStyle> _entries;

  @override
  TextStyle? operator [](Object? key) {
    if (key is! String) return null;
    final direct = _entries[key];
    if (direct != null) return direct;
    final dot = key.indexOf('.');
    if (dot != -1) {
      return _entries[key.substring(0, dot)];
    }
    return null;
  }

  @override
  void operator []=(String key, TextStyle value) => _entries[key] = value;

  @override
  void clear() => _entries.clear();

  @override
  Iterable<String> get keys => _entries.keys;

  @override
  TextStyle? remove(Object? key) => _entries.remove(key);
}

Map<String, TextStyle> _buildTheme(OcColors colors) {
  final w700 = OcText.mono.copyWith(
    color: colors.text,
    fontWeight: FontWeight.w700,
  );
  final w500 = OcText.mono.copyWith(
    color: colors.text,
    fontWeight: FontWeight.w500,
  );
  final sec = OcText.mono.copyWith(color: colors.textSecondary);
  final muted = OcText.mono.copyWith(color: colors.textMuted);

  return _PrefixThemeMap({
    'keyword': w700,
    'selector-tag': w700,
    'section': w700,
    'doctag': w700,
    'title': w500,
    'built_in': w500,
    'type': w500,
    'tag': w500,
    'name': w500,
    'attr': w500,
    'string': sec,
    'regexp': sec,
    'number': sec,
    'literal': sec,
    'symbol': sec,
    'meta': sec,
    'attribute': sec,
    'comment': muted,
    'quote': muted,
  });
}

/// Highlights [code] according to [language] using shades of grey (§7.3).
///
/// Returns null if [language] is unknown or null, or if [code] exceeds 20000 characters.
InlineSpan? highlightedCode(String code, String? language, OcColors colors) {
  if (language == null || language.isEmpty) return null;
  if (code.length > 20000) return null;

  final hl = _getHighlight();
  if (hl.getLanguage(language) == null) return null;

  final key = _CacheKey(code, language, colors);
  if (_cache.containsKey(key)) {
    final cached = _cache.remove(key)!;
    _cache[key] = cached;
    return cached;
  }

  final baseStyle = OcText.mono.copyWith(color: colors.text);
  final theme = _buildTheme(colors);
  final renderer = TextSpanRenderer(baseStyle, theme);
  final result = hl.highlight(code: code, language: language);
  result.render(renderer);
  final span = renderer.span;
  if (span == null) return null;

  if (_cache.length >= 64) {
    _cache.remove(_cache.keys.first);
  }
  _cache[key] = span;
  return span;
}
