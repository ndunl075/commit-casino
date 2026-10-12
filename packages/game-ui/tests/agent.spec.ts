import { expect, test } from '@playwright/test';

test('verification gate keeps agent wagering unavailable', async ({ page }) => {
  await page.goto('/?scenario=agent-disabled');
  await page.getByRole('button', { name: 'Agent predictions', exact: true }).click();
  await expect(page.getByText('Verification gate has not passed.', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Arm for next turn' })).toBeDisabled();
  expect(await page.evaluate(() => window.regression.agentRequests)).toEqual([]);
});
test('arm locks a stake, bound turn can be explicitly voided for a refund', async ({ page }) => {
  await page.goto('/?scenario=agent-active');
  await page.getByRole('button', { name: 'Agent predictions', exact: true }).click();
  await page.getByRole('button', { name: 'Comeback', exact: false }).click();
  await page.getByRole('button', { name: 'Arm for next turn' }).click();
  await expect(page.getByRole('button', { name: 'Cancel & refund' })).toBeVisible();
  await expect(page.getByTestId('wallet-balance')).toHaveText('9,900');
  await expect(page.getByRole('button', { name: 'Arm for next turn' })).toBeDisabled();
  await page.evaluate(() => window.regression.bindAgent());
  await page.getByRole('button', { name: 'Void & refund' }).click();
  await page.getByRole('button', { name: 'Confirm refund' }).click();
  await expect(page.getByText('Unknown · stake refunded', { exact: true })).toBeVisible();
  await expect(page.getByTestId('wallet-balance')).toHaveText('10,000');
  await expect(page.getByRole('img', { name: 'Recorded category: Unknown' })).toBeVisible();
});
test('uncertain agent command retains its request ID across dialog dismissal', async ({ page }) => {
  await page.goto('/?scenario=agent-retry');
  await page.getByRole('button', { name: 'Agent predictions', exact: true }).click();
  await page.getByRole('button', { name: 'Arm for next turn' }).click();
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText('Agent response lost after debit');
  await page.getByRole('button', { name: 'Close dialog' }).click();
  await page.getByRole('button', { name: 'Agent predictions', exact: true }).click();
  await page.getByRole('button', { name: 'Retry request' }).click();
  await expect(page.getByRole('button', { name: 'Cancel & refund' })).toBeVisible();
  await expect(page.getByTestId('wallet-balance')).toHaveText('9,900');
  const requests = await page.evaluate(() => window.regression.agentRequests);
  expect(requests).toHaveLength(2); expect(requests[0]).toBe(requests[1]);
});
