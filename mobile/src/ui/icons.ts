// The only icons the app uses (design §9.4): SF Symbols on iOS, Material
// Symbols on Android, one name for both.
import type { SymbolViewProps } from 'expo-symbols';

type Pair = Exclude<SymbolViewProps['name'], string>;

export const icons = {
  servers: { ios: 'bubble.left.and.bubble.right', android: 'forum' },
  you: { ios: 'person.crop.circle', android: 'account_circle' },
  add: { ios: 'plus', android: 'add' },
  textChannel: { ios: 'number', android: 'tag' },
  voiceChannel: { ios: 'speaker.wave.2', android: 'volume_up' },
  sidebar: { ios: 'sidebar.left', android: 'menu' },
  back: { ios: 'chevron.left', android: 'arrow_back' },
  more: { ios: 'ellipsis', android: 'more_horiz' },
  search: { ios: 'magnifyingglass', android: 'search' },
  send: { ios: 'arrow.up', android: 'arrow_upward' },
  photo: { ios: 'photo', android: 'image' },
  file: { ios: 'doc', android: 'description' },
  copy: { ios: 'doc.on.doc', android: 'content_copy' },
  edit: { ios: 'pencil', android: 'edit' },
  delete: { ios: 'trash', android: 'delete' },
  close: { ios: 'xmark', android: 'close' },
  check: { ios: 'checkmark', android: 'check' },
  clock: { ios: 'clock', android: 'schedule' },
  warning: { ios: 'exclamationmark.triangle.fill', android: 'warning' },
  lock: { ios: 'lock.fill', android: 'lock' },
  mic: { ios: 'mic.fill', android: 'mic' },
  micOff: { ios: 'mic.slash.fill', android: 'mic_off' },
  deafen: { ios: 'speaker.slash.fill', android: 'headset_off' },
  leave: { ios: 'phone.down.fill', android: 'call_end' },
  music: { ios: 'music.note', android: 'music_note' },
  play: { ios: 'play.fill', android: 'play_arrow' },
  pause: { ios: 'pause.fill', android: 'pause' },
  next: { ios: 'forward.fill', android: 'skip_next' },
  previous: { ios: 'backward.fill', android: 'skip_previous' },
  shuffle: { ios: 'shuffle', android: 'shuffle' },
  repeat: { ios: 'repeat', android: 'repeat' },
  repeatOne: { ios: 'repeat.1', android: 'repeat_one' },
  upload: { ios: 'square.and.arrow.up', android: 'upload' },
  retry: { ios: 'arrow.clockwise', android: 'refresh' },
  volume: { ios: 'speaker.wave.2.fill', android: 'volume_up' },
  volumeOff: { ios: 'speaker.slash.fill', android: 'volume_off' },
  share: { ios: 'square.and.arrow.up', android: 'share' },
  jumpDown: { ios: 'chevron.down', android: 'keyboard_arrow_down' },
} satisfies Record<string, Pair>;

export type IconName = keyof typeof icons;
