import 'package:flutter/widgets.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/ui/theme/oc_icons.dart';

/// Files whose text the Preview button shows, up to [maxPreviewBytes].
const textPreviewExtensions = {
  'txt',
  'md',
  'log',
  'json',
  'csv',
  'xml',
  'yaml',
  'yml',
  'toml',
  'ini',
  'cfg',
  'conf',
  'rs',
  'py',
  'js',
  'ts',
  'dart',
  'c',
  'h',
  'cpp',
  'java',
  'go',
  'sh',
};

const int maxPreviewBytes = 256 * 1024;

/// The lowercase text after a filename's last dot, or ''.
String fileExtension(String filename) {
  final dot = filename.lastIndexOf('.');
  return dot == -1 ? '' : filename.substring(dot + 1).toLowerCase();
}

/// A file's icon: by the kind the server gave it, else by its extension
/// (files being staged or sent have no kind yet).
IconData fileIcon(String filename, {AttachmentKind? kind}) {
  switch (kind) {
    case AttachmentKind.image:
      return OcIcons.image;
    case AttachmentKind.video:
      return OcIcons.movie;
    case AttachmentKind.audio || AttachmentKind.voiceMessage:
      return OcIcons.audioFile;
    case AttachmentKind.file || null:
      break;
  }
  final ext = fileExtension(filename);
  return switch (ext) {
    'png' ||
    'jpg' ||
    'jpeg' ||
    'gif' ||
    'webp' ||
    'svg' ||
    'bmp' ||
    'ico' => OcIcons.image,
    'mp4' || 'webm' || 'mkv' || 'mov' || 'avi' => OcIcons.movie,
    'mp3' || 'ogg' || 'wav' || 'flac' || 'm4a' || 'opus' => OcIcons.audioFile,
    'zip' ||
    '7z' ||
    'rar' ||
    'tar' ||
    'gz' ||
    'bz2' ||
    'xz' ||
    'tgz' => OcIcons.folderZip,
    _ when textPreviewExtensions.contains(ext) => OcIcons.article,
    _ => OcIcons.description,
  };
}
