import { Link } from 'expo-router';
import { Text, View, StyleSheet } from 'react-native';
import { productName } from '../src/product';
export default function Home() { return <View style={styles.page}><Text accessibilityRole="header" style={styles.title}>{productName}</Text><Text>Your app is ready to take shape.</Text><Link href="/account" style={styles.link}>Your account</Link></View>; }
const styles = StyleSheet.create({ page: { flex: 1, padding: 24, gap: 20, backgroundColor: '#faf9f6' }, title: { fontSize: 28, fontWeight: '600' }, link: { color: '#994323', paddingVertical: 16 } });
