// Isolated browser smoke test; no real archives, cloud connections or Steam scan.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const screenshots = fs.mkdtempSync(path.join(os.tmpdir(), 'chronicle-language-'));
  try {
    const context = await browser.newContext({ viewport: { width: 1024, height: 768 }, reducedMotion: 'reduce' });
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    const origin = process.env.CHRONICLE_TEST_URL || 'http://127.0.0.1:1432';
    await page.goto(origin);
    await page.locator('#tutorial-title').waitFor();
    await page.getByRole('button', { name: 'English', exact: true }).click();
    await page.waitForFunction(() => document.documentElement.lang === 'en');
    assert.match(await page.locator('#tutorial-title').innerText(), /^[^\p{Script=Han}]+$/u);
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('chronicle.app-settings.v2')).language), 'en');
    await page.screenshot({ path: path.join(screenshots, 'welcome-en.png') });
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.waitForFunction(() => [...document.querySelectorAll('.welcome-greetings span')].some(el => el.textContent === 'Hola' && Number(getComputedStyle(el).opacity) > .9), null, { timeout: 14000 });
    await page.emulateMedia({ reducedMotion: 'reduce' });
    assert.equal(await page.locator('.welcome-greetings span').first().evaluate(el => getComputedStyle(el).animationName), 'none');
    await page.getByRole('button', { name: 'Continue', exact: true }).click();
    // Skip through the real tutorial button, not by mutating component state.
    await page.locator('.tutorial-link').filter({ hasText: /Skip/ }).first().click();
    await page.locator('.toolbar button').last().click();
    await page.getByRole('button', { name: 'Interface language', exact: true }).click();
    await page.getByRole('option', { name: '简体中文', exact: true }).click();
    await page.waitForFunction(() => document.documentElement.lang === 'zh-CN');
    await page.getByRole('button', { name: '界面语言', exact: true }).click();
    await page.getByRole('option', { name: 'English', exact: true }).click();
    await page.waitForFunction(() => document.documentElement.lang === 'en');
    await page.screenshot({ path: path.join(screenshots, 'settings-en.png') });
    await page.reload();
    await page.waitForFunction(() => document.documentElement.lang === 'en');
    await page.locator('.toolbar button').last().click();
    // Inspect each real settings section in English at a compact desktop size.
    for (const button of await page.locator('.settings-dialog nav button').all()) {
      await button.click();
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    }
    await page.keyboard.press('Escape');
    await page.locator('[data-tour="create-entry"]').click();
    await page.screenshot({ path: path.join(screenshots, 'create-en.png') });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    await page.keyboard.press('Escape');
    await page.locator('[data-tour="steam-entry"]').click();
    await page.getByRole('dialog').waitFor();
    await page.screenshot({ path: path.join(screenshots, 'steam-en.png') });
    await page.evaluate(async () => {
      const { applyAppearance } = await import('/src/services/appearance.ts');
      applyAppearance({ colorTheme: 'indigo', colorMode: 'dark' });
    });
    await page.setViewportSize({ width: 820, height: 700 });
    await page.screenshot({ path: path.join(screenshots, 'steam-en-dark.png') });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    assert.deepEqual(errors, []);
    console.log(`Language smoke test passed. Screenshots: ${screenshots}`);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
