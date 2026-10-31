import { chrome } from '@/features/servers/chrome';
import { ServerListScreen } from '@/features/servers/ServerListScreen';

export default function ServersScreen() {
  return <ServerListScreen searchable={chrome.serversSearch} voiceBar={chrome.serversVoiceBar} />;
}
