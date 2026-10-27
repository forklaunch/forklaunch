import { Stack } from 'expo-router';
import { productName } from '../src/product';
export default function Layout() { return <Stack screenOptions={{ headerTitle: productName }}><Stack.Screen name="index"/><Stack.Screen name="account" options={{ title: 'Your account' }}/></Stack>; }
