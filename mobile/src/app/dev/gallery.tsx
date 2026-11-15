// Every UI primitive in each of its states, for screenshots (plan Task 6),
// and the settings kit on its own page (phone P0 Task 4): open it with
// `opencord-app:///dev/gallery?page=settings`. Development builds only.
import { router, useLocalSearchParams, useNavigation } from 'expo-router';
import { useCallback, useLayoutEffect, useState, type ReactNode } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import {
  Avatar,
  Badge,
  Banner,
  Button,
  Divider,
  Icon,
  IconButton,
  Menu,
  PaneHeader,
  ProgressBar,
  Row,
  SearchField,
  Segmented,
  ServerIcon,
  SettingsList,
  SettingsRow,
  SettingsSection,
  Slider,
  Switch,
  Text,
  fonts,
  nativeHeaderLook,
  space,
  typography,
  useSaveButton,
  useTheme,
  type IconName,
  type TextVariant,
} from '@/ui';
import { icons } from '@/ui/icons';

const PAGES = ['Primitives', 'Settings kit'];

const NOTIFY_OPTIONS = [
  { key: 'all', label: 'All messages' },
  { key: 'mentions', label: 'Only mentions' },
  { key: 'none', label: 'Nothing' },
];

interface KitDraft {
  name: string;
  description: string;
  notify: string;
  joinMessages: boolean;
  uploadMb: number;
}

const KIT_SAVED: KitDraft = {
  name: 'Night Owls',
  description: '',
  notify: 'mentions',
  joinMessages: true,
  uploadMb: 25,
};

/** How long the gallery's pretend save takes. */
const KIT_SAVE_MS = 800;

/**
 * One of each settings row in two sections, under a native header with
 * Save, as a server settings page will be (Task 7 puts them in a sheet).
 */
function SettingsKitPage({ onPage }: { onPage: (index: number) => void }) {
  const { colors, scheme } = useTheme();
  const navigation = useNavigation();
  const [saved, setSaved] = useState(KIT_SAVED);
  const [draft, setDraft] = useState(KIT_SAVED);
  const [saving, setSaving] = useState(false);
  const edit = (change: Partial<KitDraft>) => setDraft((current) => ({ ...current, ...change }));
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);

  const onSave = useCallback(async () => {
    setSaving(true);
    await new Promise((resolve) => setTimeout(resolve, KIT_SAVE_MS));
    setSaved(draft);
    setSaving(false);
  }, [draft]);
  useSaveButton({ dirty, saving, onSave });

  // The gallery hides the stack's header; this page shows it for Save.
  useLayoutEffect(() => {
    navigation.setOptions({
      headerShown: true,
      title: 'Settings kit',
      headerShadowVisible: false,
      ...nativeHeaderLook(scheme, colors.chat),
      headerTintColor: colors.text,
      headerTitleStyle: { fontFamily: fonts.semiBold, color: colors.text },
    });
    return () => navigation.setOptions({ headerShown: false });
  }, [navigation, scheme, colors]);

  return (
    <SettingsList>
      <View style={styles.pages}>
        <Segmented values={PAGES} index={1} onChange={onPage} />
      </View>
      <SettingsSection title="Overview" footer="Everyone on the server sees its name and description.">
        <SettingsRow kind="text" label="Server name" value={draft.name} placeholder="Name" maxLength={100} onChange={(name) => edit({ name })} />
        <SettingsRow
          kind="text"
          label="Description"
          value={draft.description}
          placeholder="What the server is for"
          maxLength={1000}
          multiline
          onChange={(description) => edit({ description })}
        />
        <SettingsRow kind="link" label="Roles" value="3" icon="lock" onPress={() => {}} />
        <SettingsRow
          kind="choice"
          label="Notifications"
          value={draft.notify}
          options={NOTIFY_OPTIONS}
          onChange={(notify) => edit({ notify })}
        />
        <SettingsRow
          kind="switch"
          label="Show join messages"
          detail="Say in the first channel when someone joins."
          value={draft.joinMessages}
          onChange={(joinMessages) => edit({ joinMessages })}
        />
      </SettingsSection>
      <SettingsSection title="Uploads">
        <SettingsRow
          kind="slider"
          label="Upload limit"
          value={draft.uploadMb}
          min={1}
          max={100}
          step={1}
          format={(mb) => `${mb} MB`}
          onChange={(uploadMb) => edit({ uploadMb })}
          onDone={(uploadMb) => edit({ uploadMb })}
        />
        <SettingsRow kind="destructive" label="Delete server" onPress={() => {}} />
      </SettingsSection>
    </SettingsList>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <View style={styles.section}>
      <Text variant="caption" color="textMuted">
        {title}
      </Text>
      {children}
    </View>
  );
}

export default function Gallery() {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const [volume, setVolume] = useState(1);
  const [duck, setDuck] = useState(true);
  const [segment, setSegment] = useState(0);
  const [query, setQuery] = useState('');
  const params = useLocalSearchParams<{ page?: string }>();
  // The page lives in the route, so a link to it works while it is open.
  const page = params.page === 'settings' ? 1 : 0;
  const setPage = (index: number) => router.setParams({ page: index === 1 ? 'settings' : 'primitives' });
  if (!__DEV__) return null;
  if (page === 1) return <SettingsKitPage onPage={setPage} />;
  return (
    <View style={{ flex: 1, backgroundColor: colors.sidebar }}>
      <PaneHeader
        title="Gallery"
        subtitle="Every primitive"
        left={{ icon: 'back', label: 'Back', onPress: () => {} }}
        right={[{ icon: 'more', label: 'More', onPress: () => {} }]}
      />
      <View style={styles.pages}>
        <Segmented values={PAGES} index={0} onChange={setPage} />
      </View>
      <ScrollView contentContainerStyle={{ paddingBottom: insets.bottom + space.xxxl }}>
        <Section title="Type">
          {(Object.keys(typography) as TextVariant[]).map((variant) => (
            <Text key={variant} variant={variant}>
              {variant} · Opencord 21:04
            </Text>
          ))}
        </Section>
        <Section title="Icons">
          <View style={styles.wrap}>
            {(Object.keys(icons) as IconName[]).map((name) => (
              <Icon key={name} name={name} size={22} />
            ))}
          </View>
        </Section>
        <Section title="People and servers">
          <View style={styles.line}>
            <Avatar name="Kai Nakamura" size={22} />
            <Avatar name="Mira" size={32} speaking />
            <Avatar name="Jonas" size={64} speaking />
            <ServerIcon name="Night Owls" />
            <ServerIcon name="Rust Club" size={40} />
          </View>
          <View style={styles.line}>
            <Badge count={3} />
            <Badge count={128} />
            <Badge count={2} mention />
          </View>
        </Section>
        <Section title="Buttons">
          <Button title="Create an identity" variant="primary" onPress={() => {}} />
          <Button title="Import a backup" variant="secondary" onPress={() => {}} />
          <Button title="Join voice" variant="voice" icon="voiceChannel" onPress={() => {}} />
          <Button title="Leave" variant="danger" onPress={() => {}} />
          <Button title="Adding…" variant="primary" loading onPress={() => {}} />
          <Button title="Can't do that" variant="primary" disabled onPress={() => {}} />
          <View style={styles.line}>
            <IconButton icon="mic" label="Mute" tone="toggle" size={56} onPress={() => {}} />
            <IconButton icon="micOff" label="Unmute" tone="toggle" active size={56} onPress={() => {}} />
            <IconButton icon="play" label="Play" tone="accent" size={64} onPress={() => {}} />
            <IconButton icon="leave" label="Leave" tone="danger" size={56} onPress={() => {}} />
            <IconButton icon="more" label="More" onPress={() => {}} />
          </View>
        </Section>
        <Section title="Rows">
          <Row accessibilityLabel="general" onPress={() => {}}>
            <Icon name="textChannel" size={18} color="textMuted" />
            <Text variant="callout" style={styles.rowText}>
              general
            </Text>
            <Badge count={4} />
          </Row>
          <Divider inset={space.lg} />
          <Row accessibilityLabel="music-requests" selected onPress={() => {}}>
            <Icon name="textChannel" size={18} />
            <Text variant="callout" style={styles.rowText}>
              music-requests (selected)
            </Text>
          </Row>
        </Section>
        <Section title="Feedback">
          <Banner text="Reconnecting…" tone="neutral" />
          <Banner text="Can't reach the voice server. Your network may block it." tone="error" action={{ title: 'Rejoin', onPress: () => {} }} />
          <ProgressBar progress={0.42} />
          <ProgressBar progress={1} tone="error" />
        </Section>
        <Section title="Inputs">
          <SearchField value={query} onChangeText={setQuery} placeholder="Search songs or paste a link" />
          <View style={styles.line}>
            <Icon name="volume" />
            <Slider value={volume} min={0} max={2} step={0.05} onChange={setVolume} accessibilityLabel="Music volume" />
            <Text variant="footnote" tabular>
              {Math.round(volume * 100)} %
            </Text>
          </View>
          <View style={styles.line}>
            <Text variant="callout" style={styles.rowText}>
              Lower music while people talk
            </Text>
            <Switch value={duck} onChange={setDuck} accessibilityLabel="Lower music while people talk" />
          </View>
          <Segmented values={['Up next', 'Search', 'Upload', 'History']} index={segment} onChange={setSegment} />
          <Menu
            actions={[
              { id: 'copy', title: 'Copy text', icon: 'copy' },
              { id: 'edit', title: 'Edit', icon: 'edit' },
              { id: 'delete', title: 'Delete', icon: 'delete', destructive: true },
            ]}
            onAction={() => {}}
          >
            <Button title="Open a menu" variant="secondary" onPress={() => {}} />
          </Menu>
        </Section>
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  pages: { paddingHorizontal: space.lg, paddingTop: space.md },
  section: { paddingHorizontal: space.lg, paddingTop: space.xxl, gap: space.md },
  wrap: { flexDirection: 'row', flexWrap: 'wrap', gap: space.md },
  line: { flexDirection: 'row', alignItems: 'center', gap: space.md },
  rowText: { flex: 1, marginLeft: space.md },
});
