import {
  array,
  boolean,
  handlers,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { BetterAuth } from '../../auth';
import { ci, tokens } from '../../bootstrapper';

const betterAuth = ci.resolve(tokens.BetterAuth) as BetterAuth;

/**
 * GET /discovery/auth-methods
 * Returns the configured authentication methods for this IAM instance (public, no auth required)
 */
export const getAuthMethods = handlers.get(
  schemaValidator,
  '/auth-methods',
  {
    name: 'GetAuthMethods',
    access: 'public',
    summary: 'Get configured authentication methods',
    responses: {
      200: {
        email: boolean,
        username: boolean,
        magicLink: boolean,
        emailOtp: boolean,
        phoneNumber: boolean,
        passkey: boolean,
        anonymous: boolean,
        sso: boolean,
        genericOAuth: boolean,
        twoFactor: boolean,
        socialProviders: array(string)
      }
    }
  },
  async (_req, res) => {
    const opts = betterAuth.options;
    const pluginIds = new Set(
      (opts.plugins ?? []).map((p: { id: string }) => p.id)
    );
    res.status(200).json({
      email: !!opts.emailAndPassword?.enabled,
      username: pluginIds.has('username'),
      magicLink: pluginIds.has('magic-link'),
      emailOtp: pluginIds.has('email-otp'),
      phoneNumber: pluginIds.has('phone-number'),
      passkey: pluginIds.has('passkey'),
      anonymous: pluginIds.has('anonymous'),
      sso: pluginIds.has('sso'),
      genericOAuth: pluginIds.has('generic-oauth'),
      twoFactor: pluginIds.has('two-factor'),
      socialProviders: Object.keys(opts.socialProviders ?? {})
    });
  }
);
