import 'package:flutter/widgets.dart';

/// The shell's frames, one key each, named as the floating UI's checks
/// (spec §6) and the prototype's `measure.json` name them.
abstract final class ShellKeys {
  static const titlebar = Key('float-titlebar');
  static const crumb = Key('float-crumb');
  static const rail = Key('float-rail');
  static const head = Key('float-head');
  static const channels = Key('float-channels');
  static const vcard = Key('float-vcard');
  static const me = Key('float-me');
  static const left = Key('float-left');
  static const splitter = Key('float-splitter');
  static const main = Key('float-main');
  static const hd = Key('float-hd');
  static const acts = Key('float-acts');
  static const pin = Key('float-pin');
  static const composer = Key('float-composer');
  static const side = Key('float-side');

  /// The right pane's content, there only while its slot is open or moving.
  static const panel = Key('float-panel');
  static const notch = Key('float-notch');
  static const paneTab = Key('float-paneTab');
  static const dock = Key('float-dock');
  static const tiles = Key('float-tiles');
  static const stage = Key('float-stage');
  static const search = Key('float-search');
  static const memSearch = Key('float-memSearch');
  static const railPop = Key('float-railPop');
  static const srvMenu = Key('float-srvMenu');
  static const scrim = Key('float-scrim');

  /// Every key by its name in §6, for the checks to measure.
  static const byName = <String, Key>{
    'titlebar': titlebar,
    'crumb': crumb,
    'rail': rail,
    'head': head,
    'channels': channels,
    'vcard': vcard,
    'me': me,
    'left': left,
    'splitter': splitter,
    'main': main,
    'hd': hd,
    'acts': acts,
    'pin': pin,
    'composer': composer,
    'side': side,
    'panel': panel,
    'notch': notch,
    'paneTab': paneTab,
    'dock': dock,
    'tiles': tiles,
    'stage': stage,
    'search': search,
    'memSearch': memSearch,
    'railPop': railPop,
    'srvMenu': srvMenu,
    'scrim': scrim,
  };
}
