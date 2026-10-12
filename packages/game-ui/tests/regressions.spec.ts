import { expect, test } from '@playwright/test';

test('reopening an already shown committed round preserves its result without replay', async ({ page }) => {
  await page.goto('/?scenario=already-shown');
  await expect(page.getByText('net chips', { exact: true })).toBeVisible();
  await expect(page.getByText('The wheel is turning', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Spinning…', exact: false })).toBeDisabled();
  expect(await page.evaluate(() => window.regression.sounds)).toEqual([]);
});

test('the real Pixi wheel renders under strict native script CSP', async ({ page }) => {
  const violations: string[] = [];
  page.on('console', message => { if (/content security policy|unsafe-eval/i.test(message.text())) violations.push(message.text()); });
  const response = await page.goto('/?scenario=observer');
  expect(response?.headers()['content-security-policy']).toContain("script-src 'self';");
  await expect(page.locator('.wheel-view canvas')).toBeVisible();
  await page.evaluate(() => window.regression.observer());
  await expect(page.getByText('The wheel is turning', { exact: true })).toBeVisible();
  await page.getByText('net chips', { exact: true }).waitFor();
  await expect(page.locator('.wheel-fallback')).toHaveCount(0);
  await expect(page.locator('.wheel-view canvas')).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  expect(violations).toEqual([]);
});

test('observer round chips take precedence without deleting the local draft', async ({ page }) => {
  await page.goto('/?scenario=observer');
  await page.locator('[data-bet-id="outside:red"]').click();
  await page.evaluate(() => window.regression.observer());
  await expect(page.locator('[data-bet-id="straight:7"] .placed-chip')).toHaveText('500');
  await expect(page.locator('[data-bet-id="outside:red"] .placed-chip')).toHaveCount(0);
  await expect(page.getByTestId('total-bet')).toHaveText('500 chips');
  await page.getByText('net chips', { exact: true }).waitFor();
  await page.evaluate(() => window.regression.settleObserver());
  await expect(page.locator('[data-bet-id="outside:red"] .placed-chip')).toHaveText('100');
  await expect(page.getByTestId('total-bet')).toHaveText('100 chips');
});
test('deferred old service cannot replace a new wallet or clear its draft', async ({ page }) => {
  await page.goto('/?scenario=swap');
  await page.locator('[data-bet-id="outside:red"]').click();
  await page.getByRole('button', { name: 'SPIN', exact: false }).click();
  await page.evaluate(() => window.regression.swap());
  await expect(page.getByTestId('wallet-balance')).toHaveText('4,321');
  await page.locator('[data-bet-id="straight:3"]').click();
  await page.evaluate(() => window.regression.resolveOld());
  await expect(page.getByTestId('wallet-balance')).toHaveText('4,321');
  await expect(page.locator('[data-bet-id="straight:3"] .placed-chip')).toHaveText('100');
  await expect(page.getByTestId('total-bet')).toHaveText('100 chips');
  expect(await page.evaluate(() => window.regression.sounds.filter(s => s === 'spin'))).toEqual([]);
});
test('settings and reset failures are announced inside the open dialog', async ({ page }) => {
  await page.goto('/?scenario=errors');
  await page.locator('[data-bet-id="straight:3"]').click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('slider').fill('0.2');
  await expect(page.getByRole('dialog').getByRole('alert')).toHaveText(/Settings write denied/);
  await page.getByRole('button', { name: 'Reset wallet to 10,000' }).click();
  await page.getByRole('button', { name: 'Reset wallet', exact: true }).click();
  await expect(page.getByRole('dialog').getByRole('alert')).toHaveText(/Reset write denied/);
  await page.getByRole('button', { name: 'Close dialog' }).click();
  await expect(page.getByTestId('total-bet')).toHaveText('100 chips');
});
test('idempotent retry preserves a presented result without stale spin audio', async ({ page }) => {
  await page.goto('/?scenario=retry');
  await page.locator('[data-bet-id="straight:7"]').click();
  await page.getByRole('button', { name: 'SPIN', exact: false }).click();
  await page.getByText('net chips', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Retry spin', exact: false }).click();
  await expect(page.getByText('net chips', { exact: true })).toBeVisible();
  await expect(page.locator('.result-number')).toHaveText('7');
  await expect(page.getByTestId('wallet-balance')).toHaveText('27,500');
  const state = await page.evaluate(() => ({ requests: window.regression.requests, sounds: window.regression.sounds }));
  expect(state.requests).toHaveLength(2); expect(state.requests[0]).toBe(state.requests[1]); expect(state.sounds.filter(s => s === 'spin')).toEqual([]);
});

test('graphics loss reveals the recorded result promptly without a new wager', async ({ page }) => {
  await page.goto('/?scenario=observer');
  await page.locator('.wheel-view canvas').waitFor();
  await page.evaluate(() => window.regression.observer());
  await expect(page.getByText('The wheel is turning', { exact: true })).toBeVisible();
  await page.locator('.wheel-view canvas').dispatchEvent('webglcontextlost');
  await expect(page.getByText('Simple wheel display', { exact: true })).toBeVisible();
  await expect(page.getByText('net chips', { exact: true })).toBeVisible({ timeout: 1200 });
  await expect(page.locator('.result-number')).toHaveText('7');
  expect(await page.evaluate(() => window.regression.requests)).toEqual([]);
});

test('retained round details and authoritative bet frequencies are accessible', async ({ page }) => {
  await page.goto('/?scenario=retry');
  await page.locator('[data-bet-id="straight:7"]').click();
  await page.getByRole('button', { name: 'SPIN', exact: false }).click();
  await page.getByText('net chips', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'History', exact: true }).click();
  await page.locator('summary').click();
  await expect(page.getByText('500 staked · 18,000 returned', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Close dialog' }).click();
  await page.getByRole('button', { name: 'Statistics', exact: true }).click();
  await expect(page.locator('.bet-frequencies li')).toHaveText('Straight 71 round');
});
