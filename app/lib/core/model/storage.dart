import 'package:flutter/foundation.dart';

enum StorageKind { local, s3 }

/// An S3-compatible bucket configuration.
@immutable
class S3Config {
  const S3Config({
    required this.endpoint,
    required this.region,
    required this.bucket,
    this.keyPrefix = '',
    this.pathStyle = false,
    this.allowHttp = false,
    this.accessKeyId = '',
    this.secretAccessKey = '',
  });

  final String endpoint;
  final String region;
  final String bucket;
  final String keyPrefix;
  final bool pathStyle;
  final bool allowHttp;
  final String accessKeyId;
  final String secretAccessKey;
}

/// A storage backend configured on the server.
@immutable
class StorageBackend {
  const StorageBackend({
    required this.id,
    required this.kind,
    required this.name,
    required this.active,
    this.s3,
    this.hasSecret = false,
    this.fileCount = 0,
    this.bytes = 0,
    this.lastError,
  });

  final int id;
  final StorageKind kind;
  final String name;
  final bool active;
  final S3Config? s3;
  final bool hasSecret;
  final int fileCount;
  final int bytes;
  final String? lastError;
}

/// A job moving all files to the active storage.
@immutable
class StorageMove {
  const StorageMove({
    required this.toStorageId,
    required this.running,
    required this.filesDone,
    required this.filesTotal,
    required this.bytesDone,
    required this.bytesTotal,
    required this.failed,
  });

  final int toStorageId;
  final bool running;
  final int filesDone;
  final int filesTotal;
  final int bytesDone;
  final int bytesTotal;
  final int failed;
}

/// Where a server keeps its files (owner only).
@immutable
class StorageInfo {
  const StorageInfo({
    required this.backends,
    this.moveStatus,
    required this.tempFreeBytes,
  });

  final List<StorageBackend> backends;
  final StorageMove? moveStatus;
  final int tempFreeBytes;
}

@immutable
class StorageTestStep {
  const StorageTestStep({
    required this.name,
    required this.ok,
    required this.detail,
  });

  final String name;
  final bool ok;
  final String detail;
}

@immutable
class StorageTestResult {
  const StorageTestResult({required this.steps, required this.ok});

  final List<StorageTestStep> steps;
  final bool ok;
}
