import { test, expect, type Page } from '@playwright/test';

// Exercises the scale-compatibility filter (see PROPOSAL_ and
// SPEC_scale_compatibility_filter.md) against the dummy fixture data
// (src/fixtures/alsProjects.ts) that renders before any root folder is
// chosen, so this suite needs no Tauri backend.

function rowFor(page: Page, projectName: string) {
  return page.locator('.scales-card-row', { hasText: projectName });
}

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await page.waitForSelector('text=Sunset Drive');
});

test('renders every project with its scale pills, no filter active', async ({ page }) => {
  await expect(page.locator('text=Clear all')).toHaveCount(0);
  for (const name of ['Sunset Drive', 'Midnight Loop', 'Untitled Set 12', 'Desert Bloom', 'Glass Corridor']) {
    await expect(rowFor(page, name)).toBeVisible();
  }
  await expect(rowFor(page, 'Sunset Drive').getByRole('button', { name: /^Major/ })).toBeVisible();
});

test('clicking a scale pill adds a chip, pins the origin row, and filters the table', async ({ page }) => {
  await rowFor(page, 'Sunset Drive').getByRole('button', { name: /^Major/ }).click();

  await expect(page.getByText('Major — from Sunset Drive')).toBeVisible();

  // Only Sunset Drive carries C Major (or an alternate of it), so it's the
  // sole remaining row, and it must be visually pinned.
  await expect(rowFor(page, 'Sunset Drive').locator('.pinned-icon')).toHaveAttribute(
    'title',
    'Pinned — source of filter',
  );
  await expect(rowFor(page, 'Midnight Loop')).toHaveCount(0);
  await expect(rowFor(page, 'Glass Corridor')).toHaveCount(0);

  // Clicking the same pill again removes the tag and un-pins/un-filters.
  await rowFor(page, 'Sunset Drive').getByRole('button', { name: /^Major/ }).click();
  await expect(page.locator('text=Clear all')).toHaveCount(0);
  await expect(rowFor(page, 'Glass Corridor')).toBeVisible();
});

test('matches through alternates, not just the primary label', async ({ page }) => {
  // Midnight Loop's primary candidate is "D Dorian", which is listed as an
  // *alternate* under Sunset Drive's "C Major" (same pitch collection).
  // Clicking it should pull Sunset Drive into the filtered results too.
  await rowFor(page, 'Midnight Loop').getByRole('button', { name: /Dorian/ }).click();

  await expect(rowFor(page, 'Midnight Loop')).toBeVisible();
  await expect(rowFor(page, 'Sunset Drive')).toBeVisible();
  await expect(rowFor(page, 'Glass Corridor')).toHaveCount(0);
});

test('the origin row un-pins only once its last tag is removed via the chip', async ({ page }) => {
  const sunsetRow = rowFor(page, 'Sunset Drive');
  await sunsetRow.getByRole('button', { name: /^Major/ }).click();
  await sunsetRow.getByRole('button', { name: /Minor Pentatonic/ }).click();

  await expect(sunsetRow.locator('.pinned-icon')).toHaveAttribute('title', 'Pinned — source of filter');

  await page.getByText('Major — from Sunset Drive').locator('..').getByRole('button', { name: /Remove Major filter/ }).click();
  await expect(rowFor(page, 'Sunset Drive').locator('.pinned-icon')).toHaveAttribute(
    'title',
    'Pinned — source of filter',
  );

  await page.getByText('Minor Pentatonic — from Sunset Drive').locator('..').getByRole('button', { name: /Remove Minor Pentatonic filter/ }).click();
  await expect(page.locator('text=Clear all')).toHaveCount(0);
});

test('the any/all toggle appears only with 2+ tags and changes matching rows', async ({ page }) => {
  const sunsetRow = rowFor(page, 'Sunset Drive');
  await sunsetRow.getByRole('button', { name: /^Major/ }).click();
  await expect(page.getByRole('button', { name: 'Match: Any' })).toHaveCount(0);

  await sunsetRow.getByRole('button', { name: /Minor Pentatonic/ }).click();
  await expect(page.getByRole('button', { name: 'Match: Any' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Match: All' })).toBeVisible();

  // Both tags originate from Sunset Drive, so 'all' still matches it.
  await page.getByRole('button', { name: 'Match: All' }).click();
  await expect(rowFor(page, 'Sunset Drive')).toBeVisible();
});

test('raising the coverage slider disables low-coverage pills and mutes their chip without removing it', async ({ page }) => {
  const sunsetRow = rowFor(page, 'Sunset Drive');
  // Also tag "Major" (85% coverage) so the row stays matched under 'any'
  // once the slider pushes "Minor Pentatonic" (40%) below threshold —
  // otherwise the row itself would drop out of the filtered results and
  // there'd be no pill left to assert against.
  await sunsetRow.getByRole('button', { name: /^Major/ }).click();
  // "Minor Pentatonic" is a 40%-coverage candidate on Sunset Drive.
  await sunsetRow.getByRole('button', { name: /Minor Pentatonic/ }).click();

  const slider = page.locator('input[type=range]');
  await slider.fill('60');

  const chip = page.getByText('Minor Pentatonic — from Sunset Drive');
  await expect(chip).toBeVisible();
  await expect(chip).toHaveCSS('text-decoration-line', 'line-through');

  const pill = sunsetRow.getByRole('button', { name: /Minor Pentatonic/ });
  await expect(pill).toBeDisabled();
});

test('no console errors during the full interaction flow', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (err) => errors.push(String(err)));
  page.on('console', (msg) => {
    if (msg.type() === 'error') errors.push(msg.text());
  });

  await rowFor(page, 'Sunset Drive').getByRole('button', { name: /^Major/ }).click();
  await page.locator('input[type=range]').fill('50');
  await page.getByRole('button', { name: 'Clear all' }).click();

  expect(errors).toEqual([]);
});
