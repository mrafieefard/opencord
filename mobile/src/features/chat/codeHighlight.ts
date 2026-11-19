import hljs from 'highlight.js/lib/core';
import langBash from 'highlight.js/lib/languages/bash';
import langC from 'highlight.js/lib/languages/c';
import langCpp from 'highlight.js/lib/languages/cpp';
import langCsharp from 'highlight.js/lib/languages/csharp';
import langCss from 'highlight.js/lib/languages/css';
import langDart from 'highlight.js/lib/languages/dart';
import langDiff from 'highlight.js/lib/languages/diff';
import langGo from 'highlight.js/lib/languages/go';
import langIni from 'highlight.js/lib/languages/ini';
import langJava from 'highlight.js/lib/languages/java';
import langJavascript from 'highlight.js/lib/languages/javascript';
import langJson from 'highlight.js/lib/languages/json';
import langKotlin from 'highlight.js/lib/languages/kotlin';
import langLua from 'highlight.js/lib/languages/lua';
import langMarkdown from 'highlight.js/lib/languages/markdown';
import langPhp from 'highlight.js/lib/languages/php';
import langPython from 'highlight.js/lib/languages/python';
import langRuby from 'highlight.js/lib/languages/ruby';
import langRust from 'highlight.js/lib/languages/rust';
import langShell from 'highlight.js/lib/languages/shell';
import langSql from 'highlight.js/lib/languages/sql';
import langSwift from 'highlight.js/lib/languages/swift';
import langTypescript from 'highlight.js/lib/languages/typescript';
import langXml from 'highlight.js/lib/languages/xml';
import langYaml from 'highlight.js/lib/languages/yaml';
import type { TextStyle } from 'react-native';
import type { ColorTokens } from '@/ui/theme/tokens';

export interface HighlightSpan {
  text: string;
  scope?: string;
  style: TextStyle;
}

interface EmitterNode {
  scope?: string;
  children?: (string | EmitterNode)[];
}

interface EmitterResult {
  _emitter?: {
    rootNode?: {
      children?: (string | EmitterNode)[];
    };
  };
}

let initialized = false;

function ensureInitialized(): void {
  if (initialized) return;
  initialized = true;

  hljs.registerLanguage('python', langPython);
  hljs.registerLanguage('javascript', langJavascript);
  hljs.registerLanguage('typescript', langTypescript);
  hljs.registerLanguage('rust', langRust);
  hljs.registerLanguage('dart', langDart);
  hljs.registerLanguage('go', langGo);
  hljs.registerLanguage('java', langJava);
  hljs.registerLanguage('kotlin', langKotlin);
  hljs.registerLanguage('swift', langSwift);
  hljs.registerLanguage('lua', langLua);
  hljs.registerLanguage('php', langPhp);
  hljs.registerLanguage('ruby', langRuby);
  hljs.registerLanguage('sql', langSql);
  hljs.registerLanguage('css', langCss);
  hljs.registerLanguage('json', langJson);
  hljs.registerLanguage('yaml', langYaml);
  hljs.registerLanguage('diff', langDiff);
  hljs.registerLanguage('markdown', langMarkdown);
  hljs.registerLanguage('c', langC);
  hljs.registerLanguage('cpp', langCpp);
  hljs.registerLanguage('csharp', langCsharp);
  hljs.registerLanguage('bash', langBash);
  hljs.registerLanguage('shell', langShell);
  hljs.registerLanguage('xml', langXml);
  hljs.registerLanguage('ini', langIni);

  hljs.registerAliases(['py'], { languageName: 'python' });
  hljs.registerAliases(['js', 'jsx'], { languageName: 'javascript' });
  hljs.registerAliases(['ts', 'tsx'], { languageName: 'typescript' });
  hljs.registerAliases(['rs'], { languageName: 'rust' });
  hljs.registerAliases(['golang'], { languageName: 'go' });
  hljs.registerAliases(['kt'], { languageName: 'kotlin' });
  hljs.registerAliases(['rb'], { languageName: 'ruby' });
  hljs.registerAliases(['yml'], { languageName: 'yaml' });
  hljs.registerAliases(['md'], { languageName: 'markdown' });
  hljs.registerAliases(['h'], { languageName: 'c' });
  hljs.registerAliases(['c++'], { languageName: 'cpp' });
  hljs.registerAliases(['cs'], { languageName: 'csharp' });
  hljs.registerAliases(['sh', 'zsh'], { languageName: 'bash' });
  hljs.registerAliases(['html'], { languageName: 'xml' });
  hljs.registerAliases(['toml'], { languageName: 'ini' });
}

function resolveStyle(scope: string | undefined, colors: ColorTokens): TextStyle {
  if (!scope) {
    return { color: colors.text };
  }
  const prefix = scope.split('.')[0];
  switch (prefix) {
    case 'keyword':
    case 'selector-tag':
    case 'section':
    case 'doctag':
      return { color: colors.text, fontWeight: '700' };

    case 'title':
    case 'built_in':
    case 'type':
    case 'tag':
    case 'name':
    case 'attr':
      return { color: colors.text, fontWeight: '500' };

    case 'string':
    case 'regexp':
    case 'number':
    case 'literal':
    case 'symbol':
    case 'meta':
    case 'attribute':
      return { color: colors.textSecondary };

    case 'comment':
    case 'quote':
      return { color: colors.textMuted };

    default:
      return { color: colors.text };
  }
}

function flatten(
  children: (string | EmitterNode)[],
  inheritedScope: string | undefined,
  colors: ColorTokens,
  out: HighlightSpan[],
): void {
  for (const child of children) {
    if (typeof child === 'string') {
      out.push({
        text: child,
        scope: inheritedScope,
        style: resolveStyle(inheritedScope, colors),
      });
    } else if (child && Array.isArray(child.children)) {
      flatten(child.children, child.scope || inheritedScope, colors, out);
    }
  }
}

const MAX_CACHE_SIZE = 64;
const cache = new Map<string, HighlightSpan[]>();

/**
 * Highlights `code` according to `language` with theme colors (§7.3).
 * Returns null if language is empty, unknown, or if code exceeds 400 lines or 20 KB.
 */
export function highlightCode(
  code: string,
  language: string | undefined,
  colors: ColorTokens,
): HighlightSpan[] | null {
  if (!language || typeof language !== 'string') return null;
  const lang = language.trim().toLowerCase();
  if (!lang) return null;

  // Cap at 400 lines or 20 KB (20,480 bytes)
  if (code.length > 20 * 1024) return null;
  if (code.split('\n').length > 400) return null;

  ensureInitialized();

  if (!hljs.getLanguage(lang)) {
    return null;
  }

  const cacheKey = `${colors.chat}:${lang}:${code}`;
  const cached = cache.get(cacheKey);
  if (cached) {
    cache.delete(cacheKey);
    cache.set(cacheKey, cached);
    return cached;
  }

  try {
    const result = hljs.highlight(code, { language: lang }) as unknown as EmitterResult;
    const rootChildren = result._emitter?.rootNode?.children;
    if (!rootChildren) return null;

    const spans: HighlightSpan[] = [];
    flatten(rootChildren, undefined, colors, spans);

    if (cache.size >= MAX_CACHE_SIZE) {
      const oldest = cache.keys().next().value;
      if (oldest !== undefined) {
        cache.delete(oldest);
      }
    }
    cache.set(cacheKey, spans);
    return spans;
  } catch {
    return null;
  }
}
