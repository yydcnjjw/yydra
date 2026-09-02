import { StyleSheet, Text, View } from 'react-native';

export default function IndexRoute() {
  return (
    <View style={styles.container} accessibilityRole="summary">
      <Text accessibilityRole="header" style={styles.title}>
        __PRODUCT_NAME__
      </Text>
      <Text style={styles.body}>Clean Yydra Product Workspace ready.</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 12,
    padding: 24,
  },
  title: {
    fontSize: 28,
    fontWeight: '700',
  },
  body: {
    fontSize: 16,
  },
});
