// Run against an isolated Vite origin; requires Playwright on NODE_PATH.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1024, height: 768 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    const origin = process.env.CHRONICLE_TEST_URL || 'http://127.0.0.1:1432';
    await page.route(origin + '/', route => route.fulfill({ contentType: 'text/html', body: '<html><head><link rel="stylesheet" href="/src/styles.css"></head><body><div id="test"></div></body></html>' }));
    await page.goto(origin);
    await page.evaluate(async () => {
      window.isTauri = true;
      window.testArchives = [];
      window.calls = [];
      window.failScan = false;
      window.cachedScan = null;
      window.testSaved = 0;
      window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (command === 'list_entries') return structuredClone(window.testArchives);
        if (command === 'load_steam_scan_results') return structuredClone(window.cachedScan);
        if (command === 'list_recycle_items' || command === 'list_diagnostics') return [];
        if (command === 'repository_info') return { path: 'D:/TestLibrary', totalBytes: 0 };
        if (command === 'scan_steam_saves') {
          if (window.failScan) throw '模拟网络错误';
          window.cachedScan = { steamPath: 'D:/Steam', scannedAt: 1789450000, libraries: ['D:/Steam'], warnings: [], databaseUpdatedAt: 1789450000, games: [
            { appId: '42', name: '测试游戏', hasRules: true, sources: [
              {path: 'D:/Saves/Game',kind: 'folder'},
              {path: 'D:/Steam/userdata/123/42/remote',kind: 'folder',user:{accountId:'123',displayName:'玩家甲'}},
              {path: 'D:/Steam/userdata/456/42/remote',kind: 'folder',user:{accountId:'456',displayName:'玩家乙'}},
            ] },
            { appId: '43', name: '未运行的游戏', hasRules: true, sources: [] },
            { appId: '44', name: '数据库未收录', hasRules: false, sources: [] },
          ] };
          return structuredClone(window.cachedScan);
        }
        if (command === 'add_entry') {
          const archive = { id: `new-archive-${window.testArchives.length}`, name: args.name, sources: args.sourcePaths.map(path => ({ path, kind: 'folder' })) };
          window.testArchives.push(archive);
          return structuredClone(archive);
        }
        if (command === 'create_snapshot') throw '模拟备份失败';
        throw `Unexpected command: ${command}`;
      } };
      const { createApp } = await import('/node_modules/.vite/deps/vue.js');
      const { default: Dialog } = await import('/src/components/SteamScanDialog.vue');
      window.mountDialog = () => { window.testApp = createApp(Dialog, { onSaved: () => window.testSaved++, onClose: () => window.testApp.unmount() }); window.testApp.mount('#test'); };
      window.mountDialog();
    });
    await page.getByRole('button', { name: '扫描游戏存档', exact: true }).waitFor();
    assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === 'scan_steam_saves').length), 0);
    await page.getByRole('button', { name: '扫描游戏存档', exact: true }).click();
    await page.getByText('发现 3 个已安装游戏', { exact: false }).waitFor();
    await page.getByRole('button', { name: '全选可添加项' }).click();
    await page.getByRole('checkbox', { name: '添加后立即备份' }).check();
    await page.getByRole('button', { name: '添加选中的 3 个存档' }).click();
    await page.getByText('存档已添加，初始备份失败', { exact: false }).waitFor();
    assert.equal(await page.evaluate(() => window.testSaved), 1);
    assert.equal(await page.getByRole('button', { name: '添加选中的 0 个存档' }).isDisabled(), true);
    await page.getByRole('button', { name: '关闭游戏存档识别' }).click();
    await page.evaluate(() => window.mountDialog());
    await page.getByText('文件夹 · 已添加', { exact: true }).first().waitFor();
    await page.getByText('玩家甲', { exact:true }).waitFor();
    await page.getByText('用户 ID：456', { exact:true }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.testArchives.map(a => a.name)), ['测试游戏 · 本机存档','测试游戏 · 玩家甲','测试游戏 · 玩家乙']);
    assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === 'scan_steam_saves').length), 1, 'reopening must load cached results without scanning');
    await page.getByRole('button', { name: '刷新扫描', exact: true }).click();
    assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === 'scan_steam_saves').length), 2);
    assert.equal(await page.evaluate(() => window.calls.filter(c => c.command === 'add_entry').length), 3);
    const screenshots = fs.mkdtempSync(path.join(os.tmpdir(), 'chronicle-steam-ui-'));
    await page.screenshot({ path: path.join(screenshots, 'light.png') });
    await page.evaluate(async () => { const { applyAppearance } = await import('/src/services/appearance.ts'); applyAppearance({colorTheme:'indigo',colorMode:'dark'}); });
    await page.setViewportSize({ width: 820, height: 700 });
    await page.screenshot({ path: path.join(screenshots, 'dark-narrow.png') });
    assert.equal(await page.evaluate(() => document.querySelector('.steam-settings').scrollWidth <= document.querySelector('.steam-settings').clientWidth), true);
    await page.evaluate(() => window.failScan = true);
    await page.getByRole('button', { name: '更新路径库并扫描' }).click();
    await page.getByRole('alert').filter({ hasText: '模拟网络错误' }).waitFor();
    assert.deepEqual(errors, []);
    const appPage = await browser.newPage({ viewport: { width: 1024, height: 768 } });
    await appPage.addInitScript(() => localStorage.setItem('chronicle.onboarding.local.v1', JSON.stringify({status:'completed',seenTips:[]})));
    await appPage.goto(origin);
    const theme = appPage.locator('.floating-theme-toggle');
    await theme.waitFor();
    const box = await theme.boundingBox();
    assert.equal(box.width, box.height);
    assert.ok(box.x > 940 && box.y > 680);
    assert.equal(await appPage.locator('.toolbar .mode-toggle').count(), 0);
    await theme.click();
    await appPage.getByRole('button', {name:'游戏存档识别',exact:true}).click();
    await appPage.getByRole('dialog', {name:'游戏存档识别',exact:true}).waitFor();
    await appPage.keyboard.press('Escape');
    assert.equal(await appPage.getByRole('dialog', {name:'游戏存档识别',exact:true}).count(), 0);
    await appPage.screenshot({path:path.join(screenshots,'main-window.png')});
    console.log(`Steam scan UI passed: selection, partial failure, retry deduplication, errors, narrow dark layout. Screenshots: ${screenshots}`);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
