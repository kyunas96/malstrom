import { test, expect } from '@playwright/test';

// Exercises the track category override flow (see
// docs/track-category-overrides-spec.md) against a mocked Tauri backend --
// no real backend is running in this suite (see scaleCompatibilityFilter.spec.ts),
// so `list_tracks`/`overlay_set`/`overlay_remove` are stubbed via
// `window.__TAURI_INTERNALS__.invoke` before the page's own scripts run.

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (cmd === 'list_tracks') {
          return [{ name: 'Weird Synth', kind: 'Midi', category: 'Other' }];
        }
        return null;
      },
    };
  });
  await page.goto('/');
  await page.waitForSelector('text=Sunset Drive');
  await page.getByRole('button', { name: 'Show Track Tags' }).first().click();
  await page.waitForSelector('text=Weird Synth');
});

test('overriding a track category shows the overridden label and reset button', async ({ page }) => {
  const select = page.locator('select');
  await expect(select).toHaveValue('Other');

  await select.selectOption('Vocals');

  await expect(page.getByText('(overridden)')).toBeVisible();
  await expect(select).toHaveValue('Vocals');
  await expect(page.getByRole('button', { name: 'Reset' })).toBeVisible();
});

test('resetting an override clears the label and restores the category', async ({ page }) => {
  await page.locator('select').selectOption('Vocals');
  await expect(page.getByText('(overridden)')).toBeVisible();

  await page.getByRole('button', { name: 'Reset' }).click();

  await expect(page.getByText('(overridden)')).toHaveCount(0);
  await expect(page.locator('select')).toHaveValue('Other');
});

test('no console errors during the override/reset flow', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (err) => errors.push(String(err)));
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(msg.text());
  });

  await page.locator('select').selectOption('Vocals');
  await page.getByRole('button', { name: 'Reset' }).click();

  expect(errors).toEqual([]);
});
