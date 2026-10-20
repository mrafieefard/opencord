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

  NetworkSettings copyWith({
    int? mainPort,
    bool? upnpEnabled,
    int? localListenerPort,
    List<String>? trustedProxies,
    List<String>? dnsResolvers,
    bool? acmeStaging,
  }) => NetworkSettings(
    mainPort: mainPort ?? this.mainPort,
    upnpEnabled: upnpEnabled ?? this.upnpEnabled,
    localListenerPort: localListenerPort ?? this.localListenerPort,
    trustedProxies: trustedProxies ?? this.trustedProxies,
    dnsResolvers: dnsResolvers ?? this.dnsResolvers,
    acmeStaging: acmeStaging ?? this.acmeStaging,
  );

  /// What differs from [saved], to send.
  NetworkSettingsChanges changesFrom(NetworkSettings saved) =>
      NetworkSettingsChanges(
        mainPort: mainPort != saved.mainPort ? mainPort : null,
        upnpEnabled: upnpEnabled != saved.upnpEnabled ? upnpEnabled : null,
        localListenerPort: localListenerPort != saved.localListenerPort
            ? localListenerPort
            : null,
        trustedProxies: listEquals(trustedProxies, saved.trustedProxies)
            ? null
            : trustedProxies,
        dnsResolvers: listEquals(dnsResolvers, saved.dnsResolvers)
            ? null
            : dnsResolvers,
        acmeStaging: acmeStaging != saved.acmeStaging ? acmeStaging : null,
      );

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
    this.domains = const [],
    this.dnsProviders = const [],
    this.acme = const AcmeAccount(),
    this.tunnel = const TunnelStatus(),
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

  /// The domains attached, in the order they were added.
  final List<Domain> domains;

  /// DNS providers connected, for the DNS challenge.
  final List<DnsProvider> dnsProviders;
  final AcmeAccount acme;

  /// The Cloudflare Tunnel this server runs.
  final TunnelStatus tunnel;

  DnsProvider? provider(DnsProviderKind kind) =>
      dnsProviders.where((provider) => provider.kind == kind).firstOrNull;

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
      other.upnpError == upnpError &&
      listEquals(other.domains, domains) &&
      listEquals(other.dnsProviders, dnsProviders) &&
      other.acme == acme &&
      other.tunnel == tunnel;

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
    Object.hashAll(domains),
    Object.hashAll(dnsProviders),
    acme,
    tunnel,
  );
}

enum TunnelKind {
  /// A random *.trycloudflare.com address; for testing.
  quick,

  /// The owner's tunnel, by its token.
  named,
}

enum TunnelState {
  off('Off'),
  starting('Starting'),
  running('Running'),
  restarting('Restarting'),
  failed('Cannot run');

  const TunnelState(this.label);

  final String label;
}

/// The Cloudflare Tunnel (Phase 3 plan §9.2).
@immutable
class TunnelStatus {
  const TunnelStatus({
    this.state = TunnelState.off,
    this.kind,
    this.hostname = '',
    this.domainId = 0,
    this.error = '',
    this.restarts = 0,
    this.cloudflaredVersion = '',
    this.installing = false,
  });

  final TunnelState state;
  final TunnelKind? kind;

  /// The tunnel's domain; a quick tunnel's once Cloudflare gave it.
  final String hostname;
  final int domainId;
  final String error;

  /// Times cloudflared was started again.
  final int restarts;

  /// The cloudflared found on the server; empty when there is none.
  final String cloudflaredVersion;

  /// Downloading cloudflared now.
  final bool installing;

  bool get on => state != TunnelState.off;
  bool get hasCloudflared => cloudflaredVersion.isNotEmpty;

  @override
  bool operator ==(Object other) =>
      other is TunnelStatus &&
      other.state == state &&
      other.kind == kind &&
      other.hostname == hostname &&
      other.domainId == domainId &&
      other.error == error &&
      other.restarts == restarts &&
      other.cloudflaredVersion == cloudflaredVersion &&
      other.installing == installing;

  @override
  int get hashCode => Object.hash(
    state,
    kind,
    hostname,
    domainId,
    error,
    restarts,
    cloudflaredVersion,
    installing,
  );
}

enum DnsProviderKind {
  cloudflare('cloudflare', 'Cloudflare', 'API token with DNS edit permission'),
  duckdns('duckdns', 'DuckDNS', 'Account token'),
  desec('desec', 'deSEC', 'API token');

  const DnsProviderKind(this.wire, this.label, this.tokenLabel);

  /// The name the server knows it by.
  final String wire;
  final String label;
  final String tokenLabel;

  static DnsProviderKind? fromWire(String wire) =>
      values.where((kind) => kind.wire == wire).firstOrNull;
}

/// A connected DNS provider; its token never leaves the server.
@immutable
class DnsProvider {
  const DnsProvider({required this.kind, required this.masked});

  final DnsProviderKind kind;

  /// The token's last characters, like `••••abcd`.
  final String masked;

  @override
  bool operator ==(Object other) =>
      other is DnsProvider && other.kind == kind && other.masked == masked;

  @override
  int get hashCode => Object.hash(kind, masked);
}

/// The server's account with Let's Encrypt (or its test environment).
@immutable
class AcmeAccount {
  const AcmeAccount({this.exists = false, this.email = '', this.termsUrl = ''});

  final bool exists;
  final String email;

  /// The subscriber agreement to accept before the first certificate.
  final String termsUrl;

  @override
  bool operator ==(Object other) =>
      other is AcmeAccount &&
      other.exists == exists &&
      other.email == email &&
      other.termsUrl == termsUrl;

  @override
  int get hashCode => Object.hash(exists, email, termsUrl);
}

enum CertificateState {
  none,
  requesting,
  valid,

  /// Valid, but renewing it fails.
  renewalFailed,

  /// None; the last request failed.
  failed,

  /// The tunnel's or the proxy's.
  provided,
  expired,
}

/// Getting a certificate, step by step (plan §7.1).
enum CertificateStep {
  account('Creating the account'),
  order('Ordering'),
  challenge('Setting up the challenge'),
  validation('Validating'),
  issuing('Issuing');

  const CertificateStep(this.label);

  final String label;
}

@immutable
class CertificateStatus {
  const CertificateStatus({
    this.state = CertificateState.none,
    this.issuer = '',
    this.notAfter,
    this.test = false,
    this.uploaded = false,
    this.step,
    this.error = '',
    this.retryAt,
  });

  final CertificateState state;
  final String issuer;
  final DateTime? notAfter;

  /// From Let's Encrypt's test environment: kept, not served.
  final bool test;

  /// The owner's own: not renewed by the server.
  final bool uploaded;

  /// While requesting.
  final CertificateStep? step;

  /// Why the last request or renewal failed.
  final String error;

  /// After too many failures: when it may be tried again.
  final DateTime? retryAt;

  bool get held =>
      state == CertificateState.valid ||
      state == CertificateState.renewalFailed ||
      state == CertificateState.expired;

  @override
  bool operator ==(Object other) =>
      other is CertificateStatus &&
      other.state == state &&
      other.issuer == issuer &&
      other.notAfter == notAfter &&
      other.test == test &&
      other.uploaded == uploaded &&
      other.step == step &&
      other.error == error &&
      other.retryAt == retryAt;

  @override
  int get hashCode => Object.hash(
    state,
    issuer,
    notAfter,
    test,
    uploaded,
    step,
    error,
    retryAt,
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

/// Where people reach a server (Phase 3 plan §7.5).
@immutable
class ConnectAddress {
  const ConnectAddress({
    required this.host,
    required this.port,
    this.trustedCertificate = false,
    this.domainId = 0,
  });

  final String host;
  final int port;

  /// Checked like a website, by its certificate authority.
  final bool trustedCertificate;

  /// The domain's id; 0 for the IP address.
  final int domainId;

  /// `host:port`, with brackets around an IPv6 address.
  String get display => host.contains(':') ? '[$host]:$port' : '$host:$port';

  @override
  bool operator ==(Object other) =>
      other is ConnectAddress &&
      other.host == host &&
      other.port == port &&
      other.trustedCertificate == trustedCertificate &&
      other.domainId == domainId;

  @override
  int get hashCode => Object.hash(host, port, trustedCertificate, domainId);
}

@immutable
class ServerAddresses {
  const ServerAddresses({this.primary, this.domains = const [], this.ip});

  final ConnectAddress? primary;

  /// Every active domain, the primary first.
  final List<ConnectAddress> domains;
  final ConnectAddress? ip;

  @override
  bool operator ==(Object other) =>
      other is ServerAddresses &&
      other.primary == primary &&
      listEquals(other.domains, domains) &&
      other.ip == ip;

  @override
  int get hashCode => Object.hash(primary, Object.hashAll(domains), ip);
}

enum DomainKind {
  own('Own domain'),
  freeSubdomain('Free subdomain'),
  tunnel('Cloudflare Tunnel'),
  reverseProxy('Reverse proxy'),
  cloudflareProxy('Cloudflare proxy');

  const DomainKind(this.label);

  final String label;

  /// Behind a tunnel or proxy: its certificate, its HTTP check.
  bool get behindProxy =>
      this == tunnel || this == reverseProxy || this == cloudflareProxy;

  /// Whether this server can hold a certificate of its own for it: one
  /// from an authority, or behind Cloudflare's proxy the origin
  /// certificate.
  bool get takesOwnCertificate => !behindProxy || this == cloudflareProxy;
}

enum DomainStatus {
  pending('Pending verification'),
  verified('Verified'),
  dnsProblem('DNS problem');

  const DomainStatus(this.label);

  final String label;
}

enum CheckResult {
  verified('Verified'),
  pointsElsewhere('Points to another address'),
  noRecord('No record'),
  cloudflareProxied('Proxied by Cloudflare'),
  partlyCorrect('Partly correct'),
  failed('Check failed'),
  confirmedByOwner('Confirmed by you');

  const CheckResult(this.label);

  final String label;

  bool get passed => this == verified || this == confirmedByOwner;
}

@immutable
class DomainCheck {
  const DomainCheck({
    required this.result,
    required this.message,
    required this.checkedAt,
    this.note = '',
    this.found = const [],
  });

  final CheckResult result;

  /// What happened, in plain words.
  final String message;

  /// Worth knowing even when it passed.
  final String note;
  final List<String> found;
  final DateTime checkedAt;

  @override
  bool operator ==(Object other) =>
      other is DomainCheck &&
      other.result == result &&
      other.message == message &&
      other.note == note &&
      listEquals(other.found, found) &&
      other.checkedAt == checkedAt;

  @override
  int get hashCode =>
      Object.hash(result, message, note, Object.hashAll(found), checkedAt);
}

@immutable
class Domain {
  const Domain({
    required this.id,
    required this.hostname,
    required this.kind,
    required this.publicPort,
    this.status = DomainStatus.pending,
    this.active = false,
    this.primary = false,
    this.problemSince,
    this.lastCheck,
    this.checking = false,
    this.users24h = 0,
    this.deactivatedAt,
    this.trustedCertificate = false,
    this.certificate = const CertificateStatus(),
  });

  final int id;
  final String hostname;
  final DomainKind kind;
  final int publicPort;
  final DomainStatus status;
  final bool active;
  final bool primary;
  final DateTime? problemSince;
  final DomainCheck? lastCheck;

  /// A check is running now.
  final bool checking;

  /// People who connected through it in the last 24 hours.
  final int users24h;
  final DateTime? deactivatedAt;

  /// Clients check it by its certificate authority.
  final bool trustedCertificate;
  final CertificateStatus certificate;

  bool get wasVerified => status != DomainStatus.pending;

  /// `host:port`, as people add the server by it.
  String get address => publicPort == 7710 || publicPort == 0
      ? hostname
      : '$hostname:$publicPort';

  @override
  bool operator ==(Object other) =>
      other is Domain &&
      other.id == id &&
      other.hostname == hostname &&
      other.kind == kind &&
      other.publicPort == publicPort &&
      other.status == status &&
      other.active == active &&
      other.primary == primary &&
      other.problemSince == problemSince &&
      other.lastCheck == lastCheck &&
      other.checking == checking &&
      other.users24h == users24h &&
      other.deactivatedAt == deactivatedAt &&
      other.trustedCertificate == trustedCertificate &&
      other.certificate == certificate;

  @override
  int get hashCode => Object.hash(
    id,
    hostname,
    kind,
    publicPort,
    status,
    active,
    primary,
    problemSince,
    lastCheck,
    checking,
    users24h,
    deactivatedAt,
    trustedCertificate,
    certificate,
  );
}

/// What to change about a domain; absent fields stay.
@immutable
class DomainUpdate {
  const DomainUpdate({
    this.active,
    this.primary,
    this.publicPort,
    this.confirm = false,
  });

  final bool? active;

  /// Only true: makes it the primary.
  final bool? primary;
  final int? publicPort;

  /// Vouches for a tunnel or proxy domain whose check could not reach it.
  final bool confirm;
}
