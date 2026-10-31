// The tab bar's search tab (iOS): the servers, filtered by the system's
// search field. The voice connection shows above the tab bar here.
import { ServerListScreen } from '@/features/servers/ServerListScreen';

export default function SearchScreen() {
  return <ServerListScreen searchable voiceBar={false} />;
}
