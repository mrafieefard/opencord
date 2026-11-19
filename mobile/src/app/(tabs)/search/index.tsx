// The tab bar's search tab (iOS): channels, servers, and members ranked by
// switcher rules (§4.9).
import { useNavigation } from 'expo-router';
import { useLayoutEffect, useState } from 'react';
import type { NativeSyntheticEvent } from 'react-native';

import { SearchResults } from '@/features/search/SearchResults';

export default function SearchScreen() {
  const [query, setQuery] = useState('');
  const navigation = useNavigation();

  useLayoutEffect(() => {
    navigation.setOptions({
      headerSearchBarOptions: {
        placeholder: 'Search',
        hideWhenScrolling: false,
        onChangeText: (event: NativeSyntheticEvent<{ text: string }>) => {
          setQuery(event.nativeEvent.text);
        },
        onCancelButtonPress: () => {
          setQuery('');
        },
      },
    });
  }, [navigation]);

  return <SearchResults query={query} />;
}

