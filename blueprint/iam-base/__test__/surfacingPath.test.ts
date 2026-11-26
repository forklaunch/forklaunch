import { surfacingPath } from '../domain/utils/surfacingPath.util';

describe('surfacingPath', () => {
  it('signs the full mounted path, not the router-relative one', () => {
    expect(surfacingPath('user-1', 'roles')).toBe('/user/user-1/surface-roles');
    expect(surfacingPath('user-1', 'permissions')).toBe(
      '/user/user-1/surface-permissions'
    );
  });

  it('encodes the subject as the SDK does in the request URL', () => {
    expect(surfacingPath('a/b c', 'roles')).toBe(
      '/user/a%2Fb%20c/surface-roles'
    );
  });
});
