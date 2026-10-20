import 'package:opencord/core/model/network.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

/// The core's view of Domains & network as the app's model.
NetworkInfo networkInfoFrom(core.NetworkInfo info) {
  final address = info.publicAddress;
  final settings = info.settings;
  return NetworkInfo(
    publicAddress: PublicAddress(
      ipv4: address.ipv4,
      ipv6: address.ipv6,
      detectedIpv4: address.detectedIpv4,
      detectedIpv6: address.detectedIpv6,
      overrideIpv4: address.overrideIpv4,
      overrideIpv6: address.overrideIpv6,
      autoDetect: address.autoDetect,
      detectedAt: address.detectedAtMs == 0
          ? null
          : DateTime.fromMillisecondsSinceEpoch(address.detectedAtMs),
      detectError: address.detectError,
      hasIpv6: address.hasIpv6,
    ),
    ports: [for (final port in info.ports) _portFrom(port)],
    settings: NetworkSettings(
      mainPort: settings.mainPort,
      upnpEnabled: settings.upnpEnabled,
      localListenerPort: settings.localListenerPort,
      trustedProxies: settings.trustedProxies,
      dnsResolvers: settings.dnsResolvers,
      acmeStaging: settings.acmeStaging,
    ),
    listeningPort: info.listeningPort,
    nextMainPort: _stateFrom(info.nextMainPort),
    upnpGateway: info.upnpGateway,
    upnpExternalIp: info.upnpExternalIp,
    upnpError: info.upnpError,
    domains: [for (final domain in info.domains) domainFrom(domain)],
    dnsProviders: [
      for (final provider in info.dnsProviders)
        if (DnsProviderKind.fromWire(provider.kind) case final kind?)
          DnsProvider(kind: kind, masked: provider.masked),
    ],
    acme: AcmeAccount(
      exists: info.acme.exists,
      email: info.acme.email,
      termsUrl: info.acme.termsUrl,
    ),
    tunnel: tunnelFrom(info.tunnel),
  );
}

TunnelStatus tunnelFrom(core.TunnelStatus tunnel) => TunnelStatus(
  state: switch (tunnel.state) {
    core.TunnelState.off => TunnelState.off,
    core.TunnelState.starting => TunnelState.starting,
    core.TunnelState.running => TunnelState.running,
    core.TunnelState.restarting => TunnelState.restarting,
    core.TunnelState.failed => TunnelState.failed,
  },
  kind: switch (tunnel.kind) {
    core.TunnelKind.quick => TunnelKind.quick,
    core.TunnelKind.named => TunnelKind.named,
    null => null,
  },
  hostname: tunnel.hostname,
  domainId: tunnel.domainId,
  error: tunnel.error,
  restarts: tunnel.restarts,
  cloudflaredVersion: tunnel.cloudflaredVersion,
  installing: tunnel.installing,
);

Domain domainFrom(core.Domain domain) => Domain(
  id: domain.id,
  hostname: domain.hostname,
  kind: switch (domain.kind) {
    core.DomainKind.own => DomainKind.own,
    core.DomainKind.freeSubdomain => DomainKind.freeSubdomain,
    core.DomainKind.tunnel => DomainKind.tunnel,
    core.DomainKind.reverseProxy => DomainKind.reverseProxy,
    core.DomainKind.cloudflareProxy => DomainKind.cloudflareProxy,
  },
  publicPort: domain.publicPort,
  status: switch (domain.status) {
    core.DomainStatus.pending => DomainStatus.pending,
    core.DomainStatus.verified => DomainStatus.verified,
    core.DomainStatus.dnsProblem => DomainStatus.dnsProblem,
  },
  active: domain.active,
  primary: domain.primary,
  problemSince: _time(domain.problemSinceMs),
  lastCheck: switch (domain.lastCheck) {
    final check? => domainCheckFrom(check),
    null => null,
  },
  checking: domain.checking,
  users24h: domain.users24H,
  deactivatedAt: _time(domain.deactivatedAtMs),
  trustedCertificate: domain.trustedCertificate,
  certificate: certificateFrom(domain.certificate),
);

CertificateStatus certificateFrom(core.CertificateStatus certificate) =>
    CertificateStatus(
      state: switch (certificate.state) {
        core.CertificateState.none => CertificateState.none,
        core.CertificateState.requesting => CertificateState.requesting,
        core.CertificateState.valid => CertificateState.valid,
        core.CertificateState.renewalFailed => CertificateState.renewalFailed,
        core.CertificateState.failed => CertificateState.failed,
        core.CertificateState.provided => CertificateState.provided,
        core.CertificateState.expired => CertificateState.expired,
      },
      issuer: certificate.issuer,
      notAfter: _time(certificate.notAfterMs),
      test: certificate.test,
      uploaded: certificate.uploaded,
      step: switch (certificate.step) {
        core.CertificateStep.account => CertificateStep.account,
        core.CertificateStep.order => CertificateStep.order,
        core.CertificateStep.challenge => CertificateStep.challenge,
        core.CertificateStep.validation => CertificateStep.validation,
        core.CertificateStep.issuing => CertificateStep.issuing,
        null => null,
      },
      error: certificate.error,
      retryAt: _time(certificate.retryAtMs),
    );

DomainCheck domainCheckFrom(core.DomainCheck check) => DomainCheck(
  result: switch (check.result) {
    core.CheckResult.verified => CheckResult.verified,
    core.CheckResult.pointsElsewhere => CheckResult.pointsElsewhere,
    core.CheckResult.noRecord => CheckResult.noRecord,
    core.CheckResult.cloudflareProxied => CheckResult.cloudflareProxied,
    core.CheckResult.partlyCorrect => CheckResult.partlyCorrect,
    core.CheckResult.failed => CheckResult.failed,
    core.CheckResult.confirmedByOwner => CheckResult.confirmedByOwner,
  },
  message: check.message,
  note: check.note,
  found: check.found,
  checkedAt: DateTime.fromMillisecondsSinceEpoch(check.checkedAtMs),
);

DateTime? _time(int ms) =>
    ms == 0 ? null : DateTime.fromMillisecondsSinceEpoch(ms);

core.DomainKind domainKindTo(DomainKind kind) => switch (kind) {
  DomainKind.own => core.DomainKind.own,
  DomainKind.freeSubdomain => core.DomainKind.freeSubdomain,
  DomainKind.tunnel => core.DomainKind.tunnel,
  DomainKind.reverseProxy => core.DomainKind.reverseProxy,
  DomainKind.cloudflareProxy => core.DomainKind.cloudflareProxy,
};

core.DomainUpdate domainUpdateTo(DomainUpdate update) => core.DomainUpdate(
  active: update.active,
  primary: update.primary,
  publicPort: update.publicPort,
  confirm: update.confirm,
);

ServerAddresses serverAddressesFrom(core.ServerAddresses addresses) =>
    ServerAddresses(
      primary: switch (addresses.primary) {
        final primary? => _addressFrom(primary),
        null => null,
      },
      domains: [for (final domain in addresses.domains) _addressFrom(domain)],
      ip: switch (addresses.ip) {
        final ip? => _addressFrom(ip),
        null => null,
      },
    );

ConnectAddress _addressFrom(core.ConnectAddress address) => ConnectAddress(
  host: address.host,
  port: address.port,
  trustedCertificate: address.trustedCertificate,
  domainId: address.domainId,
);

PortStatus _portFrom(core.PortStatus port) => PortStatus(
  purpose: switch (port.purpose) {
    core.PortPurpose.main => PortPurpose.main,
    core.PortPurpose.voice => PortPurpose.voice,
    core.PortPurpose.music => PortPurpose.music,
    core.PortPurpose.acmeHttp => PortPurpose.acmeHttp,
    core.PortPurpose.localProxy => PortPurpose.localProxy,
  },
  protocol: switch (port.protocol) {
    core.PortProtocol.tcp => PortProtocol.tcp,
    core.PortProtocol.udp => PortProtocol.udp,
  },
  port: port.port,
  state: _stateFrom(port.state),
  mapping: switch (port.mapping) {
    core.MappingState.notUsed => MappingState.notUsed,
    core.MappingState.mapped => MappingState.mapped,
    core.MappingState.failed => MappingState.failed,
  },
  mappingError: port.mappingError,
);

PortState _stateFrom(core.PortState state) => switch (state) {
  core.PortState.unknown => PortState.unknown,
  core.PortState.listening => PortState.listening,
  core.PortState.off => PortState.off,
  core.PortState.free => PortState.free,
  core.PortState.inUse => PortState.inUse,
  core.PortState.notPermitted => PortState.notPermitted,
};

core.NetworkSettingsChanges networkChangesTo(NetworkSettingsChanges changes) =>
    core.NetworkSettingsChanges(
      mainPort: changes.mainPort,
      upnpEnabled: changes.upnpEnabled,
      localListenerPort: changes.localListenerPort,
      trustedProxies: changes.trustedProxies,
      dnsResolvers: changes.dnsResolvers,
      acmeStaging: changes.acmeStaging,
    );

ServerNotice serverNoticeFrom(core.ServerNotice notice) => ServerNotice(
  key: notice.key,
  title: notice.title,
  body: notice.body,
  severity: switch (notice.severity) {
    core.NoticeSeverity.info => NoticeSeverity.info,
    core.NoticeSeverity.warning => NoticeSeverity.warning,
  },
);
