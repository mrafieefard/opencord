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
  );
}

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
