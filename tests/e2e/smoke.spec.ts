import { expect, test } from '@playwright/test';

test('serves the room creation page', async ({ page }) => {
  const response = await page.goto('/');

  expect(response?.status()).toBe(200);
  await expect(page.getByRole('heading', { name: '投票部屋を作る' })).toBeVisible();
});
