import { describe, expect, it } from 'vitest';
import { collidesWithOriginal } from './fileName';

describe('collidesWithOriginal', () => {
  const original = '/Users/me/Projects/MyProject.als';

  it('flags an exact match', () => {
    expect(collidesWithOriginal('MyProject.als', original)).toBe(true);
  });

  it('flags a match missing the extension', () => {
    expect(collidesWithOriginal('MyProject', original)).toBe(true);
  });

  it('flags a case-insensitive match', () => {
    expect(collidesWithOriginal('myproject.ALS', original)).toBe(true);
  });

  it('allows a distinct name', () => {
    expect(collidesWithOriginal('MyProject (D Minor).als', original)).toBe(false);
  });
});
