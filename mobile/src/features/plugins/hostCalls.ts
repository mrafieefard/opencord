// React Native's answers to a plugin area's host calls (plugins M4 plan,
// Tasks 9 and 11): the phone's pickers, dialog, clipboard, navigation and
// audio, and the core's files. Each reply is `{"ok":…}` or
// `{"error":"<kind>"}`; a call for another area, or after stop, is dropped.
import { AttachmentVariant } from '@opencord/core-native';

import type { CoreApi } from '@/core/api';
import { errorKind } from '@/core/errors';

import type { AreaModule, ViewSession } from './viewSession';

export interface HostCallDeps {
  core: Pick<CoreApi, 'attachmentFile' | 'pluginImageFile' | 'pluginAssetFile' | 'viewUpload' | 'avatarFile'>;
  serverKey: string;
  pluginId: string;
  userName(id: string): string | undefined;
  userAvatarHash(id: string): string | null | undefined;
  channelName(id: string): string | undefined;
  /** A chosen image's path; null when cancelled. */
  pickImage(): Promise<string | null>;
  /** A chosen file's path, of the `accept` types (all when empty); null when cancelled. */
  pickDocument(accept: string[]): Promise<string | null>;
  /** Asks the viewer, then opens it; whether it opened. */
  confirmLink(url: string): Promise<boolean>;
  /** The clipboard, then the toast `Copied`. */
  copy(text: string): Promise<void>;
  openChannel(id: string): void;
  /** Plays the file; resolves when it ends. */
  playSound(path: string, volume: number): Promise<void>;
}

type Args = Record<string, unknown>;

class UnknownMethod extends Error {
  override name = 'unknown_method';
}

class BadArgs extends Error {
  override name = 'bad_args';
}

function text(args: Args, field: string): string {
  const value = args[field];
  if (typeof value === 'string') return value;
  if (typeof value === 'number') return String(value);
  throw new BadArgs(field);
}

function strings(args: Args, field: string): string[] {
  const value = args[field];
  if (value === undefined) return [];
  if (!Array.isArray(value)) throw new BadArgs(field);
  return value.map(String);
}

async function answer(method: string, args: Args, session: Pick<ViewSession, 'viewId'>, deps: HostCallDeps): Promise<unknown> {
  const { core, serverKey, pluginId } = deps;
  switch (method) {
    case 'image': {
      const source = args.source;
      if (source === null || typeof source !== 'object') throw new BadArgs('source');
      const attachment = (source as Args).attachment;
      const path =
        attachment !== undefined
          ? await core.attachmentFile(serverKey, String(attachment), AttachmentVariant.Original)
          : await core.pluginImageFile(serverKey, pluginId, JSON.stringify(source));
      return { path };
    }
    case 'asset':
      return { path: await core.pluginAssetFile(serverKey, pluginId, text(args, 'path')) };
    case 'user': {
      const id = text(args, 'id');
      const hash = deps.userAvatarHash(id);
      let avatar: string | null = null;
      if (hash) {
        try {
          avatar = await core.avatarFile(serverKey, hash);
        } catch (error: unknown) {
          console.warn(`plugin user avatar failed: ${errorKind(error)}`);
        }
      }
      return { name: deps.userName(id) ?? '', avatar_path: avatar };
    }
    case 'names': {
      const users: Record<string, string> = {};
      const channels: Record<string, string> = {};
      for (const id of strings(args, 'users')) {
        const name = deps.userName(id);
        if (name !== undefined) users[id] = name;
      }
      for (const id of strings(args, 'channels')) {
        const name = deps.channelName(id);
        if (name !== undefined) channels[id] = name;
      }
      return { users, channels };
    }
    case 'pick': {
      const path = args.image === true ? await deps.pickImage() : await deps.pickDocument(strings(args, 'accept'));
      return path === null ? {} : { path };
    }
    case 'upload': {
      const nodeId = args.node_id;
      if (typeof nodeId !== 'number' || !Number.isInteger(nodeId) || nodeId < 0) throw new BadArgs('node_id');
      const viewId = session.viewId;
      if (viewId === null) throw new BadArgs('view');
      await core.viewUpload(viewId, BigInt(nodeId), text(args, 'path'));
      return null;
    }
    case 'link':
      return { opened: await deps.confirmLink(text(args, 'url')) };
    case 'copy':
      await deps.copy(text(args, 'text'));
      return null;
    case 'open_channel':
      deps.openChannel(text(args, 'id'));
      return null;
    case 'sound': {
      const volume = typeof args.volume === 'number' ? Math.min(Math.max(args.volume, 0), 1) : 1;
      const path = await core.pluginAssetFile(serverKey, pluginId, text(args, 'asset'));
      await deps.playSound(path, volume);
      return null;
    }
    default:
      throw new UnknownMethod(method);
  }
}

/** Answers `session`'s host calls until the returned function is called. */
export function answerHostCalls(
  session: Pick<ViewSession, 'areaId' | 'viewId'>,
  area: Pick<AreaModule, 'addHostCallListener' | 'hostReply'>,
  deps: HostCallDeps,
): () => void {
  let stopped = false;
  const subscription = area.addHostCallListener((event) => {
    if (stopped || event.areaId !== session.areaId) return;
    const reply = (json: string) => {
      if (!stopped) area.hostReply(session.areaId, event.id, json);
    };
    let args: Args;
    try {
      const parsed: unknown = JSON.parse(event.json);
      args = parsed !== null && typeof parsed === 'object' ? (parsed as Args) : {};
    } catch {
      reply(JSON.stringify({ error: 'bad_args' }));
      return;
    }
    answer(event.method, args, session, deps).then(
      (value) => reply(JSON.stringify({ ok: value })),
      (error: unknown) => {
        const kind = errorKind(error);
        console.warn(`plugin host call ${event.method} failed: ${kind}`);
        reply(JSON.stringify({ error: kind }));
      },
    );
  });
  return () => {
    stopped = true;
    subscription.remove();
  };
}
