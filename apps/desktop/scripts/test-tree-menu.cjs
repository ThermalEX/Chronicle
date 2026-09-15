// Isolated browser regression: requires Playwright on NODE_PATH and a running Vite server.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    await page.addInitScript(() => localStorage.setItem('chronicle.onboarding.local.v1', JSON.stringify({ status: 'skipped', seenTips: [] })));
    await page.goto(process.env.CHRONICLE_TEST_URL || 'http://127.0.0.1:1427');
    await page.locator('.add-button').waitFor();
    await page.evaluate(async () => {
      const { archiveRepository: repo } = await import('/src/services/repository.ts');
      const category = await repo.createCategory('分类甲');
      await repo.createCategory('分类乙');
      await repo.createArchive({ name: '测试存档', categoryId: category.id, sources: [{ id: 'source', path: 'test.txt', name: 'test.txt', kind: 'file' }], storagePolicy: 'local', syncMode: 'manual', autoBackupEnabled: false, automaticUploadEnabled: false, createInitialSnapshot: false });
    });
    await page.reload();
    const buttons = page.getByRole('button', { name: '分类操作', exact: true });
    const menu = page.locator('.tree-menu');
    await buttons.first().click(); await menu.waitFor();
    await page.locator('.titlebar .brand').click();
    await menu.waitFor({ state: 'detached', timeout: 2000 });
    await buttons.first().click(); await menu.waitFor();
    await buttons.first().click(); await menu.waitFor({ state: 'detached' });
    await buttons.first().click();
    await buttons.nth(1).click();
    assert.equal(await menu.count(), 1);
    await menu.locator('label').click({ position: { x: 5, y: 5 } });
    assert.equal(await menu.count(), 1, 'menu content is not an outside click');
    await page.keyboard.press('Escape');
    await menu.waitFor({ state: 'detached' });
    await buttons.first().click();
    const target = await menu.locator('.trigger').getAttribute('aria-label');
    const destination = target.includes('分类甲') ? '分类乙' : '分类甲';
    await menu.locator('.trigger').click();
    await page.getByRole('option', { name: destination, exact: true }).click();
    await menu.waitFor({ state: 'detached' });
    assert.equal(await page.evaluate(async () => {
      const { archiveRepository: repo } = await import('/src/services/repository.ts');
      return (await repo.listCategories()).filter(item => item.parentId).length;
    }), 1, 'teleported choice actually moves the category');
    await page.getByRole('button', { name: `展开 ${destination}`, exact: true }).click();
    const nestedCategory = page.getByRole('button', { name: '展开 分类甲', exact: true });
    if (await nestedCategory.isVisible()) await nestedCategory.click();
    const archiveButton = page.getByRole('button', { name: '存档移动操作', exact: true });
    await archiveButton.click(); await menu.waitFor();
    await menu.locator('.trigger').click();
    await page.getByRole('option', { name: destination, exact: true }).click();
    await menu.waitFor({ state: 'detached' });
    assert.equal(await page.evaluate(async () => {
      const { archiveRepository: repo } = await import('/src/services/repository.ts');
      return Boolean((await repo.listArchives())[0].categoryId);
    }), true);
    console.log('Tree menu: outside click, trigger toggle, switching, Escape, category and archive moves passed.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
