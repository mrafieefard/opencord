/// Domains & network (Phase 3 plan §5, §8): the server's public address,
/// its ports and how they are reached. The owner's only.
library;

import 'package:flutter/foundation.dart';

/// Where the server is reached from the internet. Addresses are empty when
/// unknown.
@immutable
class PublicAddress {
  const PublicAddress({
    this.ipv4 = '',
    this.ipv6 = '',
    this.detectedIpv4 = '',
    this.detectedIpv6 = '',
    this.overrideIpv4 = '',
    this.overrideIpv6 = '',
    this.autoDetect = true,
    this.detectedAt,
    this.detectError = '',
    this.hasIpv6 = false,
  });

  /// In use: the override when there is one, else what was detected.
  final String ipv4;
  final String ipv6;
  final String detectedIpv4;
  final String detectedIpv6;
  final String overrideIpv4;
  final String overrideIpv6;
  final bool autoDetect;
  final DateTime? detectedAt;

  /// Why the last detection failed, if it did.
  final String detectError;

  /// Whether the server's machine has IPv6 at all.
  final bool hasIpv6;

  bool get overridden => overrideIpv4.isNotEmpty || overrideIpv6.isNotEmpty;

  @override
  bool operator ==(Object other) =>
      other is PublicAddress &&
      other.ipv4 == ipv4 &&
      other.ipv6 == ipv6 &&
      other.detectedIpv4 == detectedIpv4 &&
      other.detectedIpv6 == detectedIpv6 &&
      other.overrideIpv4 == overrideIpv4 &&
      other.overrideIpv6 == overrideIpv6 &&
      other.autoDetect == autoDetect &&
      other.detectedAt == detectedAt &&
      other.detectError == detectError &&
      other.hasIpv6 == hasIpv6;

  @override
  int get hashCode => Object.hash(
    ipv4,
    ipv6,
    detectedIpv4,
    detectedIpv6,
    overrideIpv4,
    overrideIpv6,
    autoDetect,
    detectedAt,
    detectError,
    hasIpv6,
  );
}

enum PortPurpose {
  main('Main', 'Chat, voice signalling, files, invites and Lavalink'),
  voice('Voice and video', 'Calls, cameras and screen shares'),
  music('Music', 'Audio from Lavalink nodes'),
  acmeHttp('Certificates', "Let's Encrypt's check, only while requesting"),
  localProxy(
    'Local listener',
    'For tunnels and reverse proxies, this machine only',
  );

  const PortPurpose(this.label, this.description);

  final String label;
  final String description;
}

enum PortProtocol { tcp, udp }

enum PortState {
  unknown('Unknown'),
  listening('Listening'),
  off('Off'),
  free('Free'),
  inUse('In use by another program'),
  notPermitted('Not allowed');

  const PortState(this.label);

  final String label;
}

enum MappingState { notUsed, mapped, failed }

@immutable
class PortStatus {
  const PortStatus({
    required this.purpose,
    required this.protocol,
    required this.port,
    required this.state,
    this.mapping = MappingState.notUsed,
    this.mappingError = '',
  });

  final PortPurpose purpose;
  final PortProtocol protocol;
  final int port;
  final PortState state;
  final MappingState mapping;
  final String mappingError;

  @override
  bool operator ==(Object other) =>
      other is PortStatus &&
      other.purpose == purpose &&
      other.protocol == protocol &&
      other.port == port &&
      other.state == state &&
      other.mapping == mapping &&
      other.mappingError == mappingError;

  @override
  int get hashCode =>
      Object.hash(purpose, protocol, port, state, mapping, mappingError);
}

@immutable
class NetworkSettings {
  const NetworkSettings({
    this.mainPort = 0,
    this.upnpEnabled = false,
    this.localListenerPort = 7719,
    this.trustedProxies = const ['127.0.0.1', '::1'],
    this.dnsResolvers = const ['1.1.1.1', '8.8.8.8', '9.9.9.9'],
    this.acmeStaging = false,
  });

  /// From the next start; 0 keeps the configured port.
  final int mainPort;
  final bool upnpEnabled;
  final int localListenerPort;
  final List<String> trustedProxies;
  final List<String> dnsResolvers;
  final bool acmeStaging;

  @override
  bool operator ==(Object other) =>
      other is NetworkSettings &&
      other.mainPort == mainPort &&
      other.upnpEnabled == upnpEnabled &&
      other.localListenerPort == localListenerPort &&
      listEquals(other.trustedProxies, trustedProxies) &&
      listEquals(other.dnsResolvers, dnsResolvers) &&
      other.acmeStaging == acmeStaging;

  @override
  int get hashCode => Object.hash(
    mainPort,
    upnpEnabled,
    localListenerPort,
    Object.hashAll(trustedProxies),
    Object.hashAll(dnsResolvers),
    acmeStaging,
  );
}

/// What to change; absent fields stay as they are.
@immutable
class NetworkSettingsChanges {
  const NetworkSettingsChanges({
    this.mainPort,
    this.upnpEnabled,
    this.localListenerPort,
    this.trustedProxies,
    this.dnsResolvers,
    this.acmeStaging,
  });

  final int? mainPort;
  final bool? upnpEnabled;
  final int? localListenerPort;
  final List<String>? trustedProxies;
  final List<String>? dnsResolvers;
  final bool? acmeStaging;
}

@immutable
class NetworkInfo {
  const NetworkInfo({
    this.publicAddress = const PublicAddress(),
    this.ports = const [],
    this.settings = const NetworkSettings(),
    this.listeningPort = 0,
    this.nextMainPort = PortState.unknown,
    this.upnpGateway = '',
    this.upnpExternalIp = '',
    this.upnpError = '',
  });

  final PublicAddress publicAddress;
  final List<PortStatus> ports;
  final NetworkSettings settings;

  /// The main port in use now.
  final int listeningPort;

  /// Whether [NetworkSettings.mainPort] can be had, when it differs.
  final PortState nextMainPort;
  final String upnpGateway;
  final String upnpExternalIp;
  final String upnpError;

  /// The main port changes on the next start.
  bool get restartPending =>
      settings.mainPort != 0 && settings.mainPort != listeningPort;

  PortStatus? port(PortPurpose purpose) =>
      ports.where((port) => port.purpose == purpose).firstOrNull;

  @override
  bool operator ==(Object other) =>
      other is NetworkInfo &&
      other.publicAddress == publicAddress &&
      listEquals(other.ports, ports) &&
      other.settings == settings &&
      other.listeningPort == listeningPort &&
      other.nextMainPort == nextMainPort &&
      other.upnpGateway == upnpGateway &&
      other.upnpExternalIp == upnpExternalIp &&
      other.upnpError == upnpError;

  @override
  int get hashCode => Object.hash(
    publicAddress,
    Object.hashAll(ports),
    settings,
    listeningPort,
    nextMainPort,
    upnpGateway,
    upnpExternalIp,
    upnpError,
  );
}

enum NoticeSeverity { info, warning }

/// Something the owner or admins should know: the public address changed,
/// a domain's DNS broke, a certificate is about to expire.
@immutable
class ServerNotice {
  const ServerNotice({
    required this.key,
    required this.title,
    required this.body,
    this.severity = NoticeSeverity.info,
  });

  /// One per subject: a newer notice replaces an older one.
  final String key;
  final NoticeSeverity severity;
  final String title;
  final String body;
}
