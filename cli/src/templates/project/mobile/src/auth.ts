import { createAuthClient } from 'better-auth/react';
import { jwtClient } from 'better-auth/client/plugins';
import { expoClient } from '@better-auth/expo/client';
import * as SecureStore from 'expo-secure-store';
import { appScheme } from './product';
// This is a public backend address, never an API key or provider credential.
const baseURL = process.env.EXPO_PUBLIC_IAM_URL;
export const auth = baseURL ? createAuthClient({ baseURL, plugins: [jwtClient(), expoClient({ scheme: appScheme, storagePrefix: appScheme, storage: SecureStore })] }) : null;
