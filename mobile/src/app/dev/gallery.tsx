// Every UI primitive in each of its states, for screenshots (plan Task 6).
// Development builds only.
import { useState, type ReactNode } from 'react';
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
  Slider,
  Switch,
  Text,
  space,
  typography,
  useTheme,
  type IconName,
  type TextVariant,
} from '@/ui';
import { icons } from '@/ui/icons';

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
  if (!__DEV__) return null;
  return (
    <View style={{ flex: 1, backgroundColor: colors.sidebar }}>
      <PaneHeader
        title="Gallery"
        subtitle="Every primitive"
        left={{ icon: 'back', label: 'Back', onPress: () => {} }}
        right={[{ icon: 'more', label: 'More', onPress: () => {} }]}
      />
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
  section: { paddingHorizontal: space.lg, paddingTop: space.xxl, gap: space.md },
  wrap: { flexDirection: 'row', flexWrap: 'wrap', gap: space.md },
  line: { flexDirection: 'row', alignItems: 'center', gap: space.md },
  rowText: { flex: 1, marginLeft: space.md },
});
