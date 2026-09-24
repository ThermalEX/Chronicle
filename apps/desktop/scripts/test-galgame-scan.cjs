// Run against an isolated Vite origin; requires Playwright on NODE_PATH.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1024, height: 720 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    const origin = process.env.CHRONICLE_TEST_URL || 'http://127.0.0.1:1432';
    await page.route(origin + '/', route => route.fulfill({ contentType: 'text/html', body: '<html><head><link rel="stylesheet" href="/src/styles.css"></head><body><div id="test"></div></body></html>' }));
    await page.goto(origin);
    await page.evaluate(async () => {
      window.isTauri = true;
      window.archives = [];
      window.calls = [];
      window.cache = null;
      window.corrupt = false;
      window.invalid = false;
      window.slowScan = false;
      window.callbacks = new Map();
      window.events = new Map();
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
      window.__TAURI_INTERNALS__ = {
        transformCallback: callback => { const id = window.callbacks.size + 1; window.callbacks.set(id, callback); return id; },
        invoke: async (command, args) => {
          window.calls.push({ command, args });
          if (command === 'plugin:event|listen') { window.events.set(args.event, args.handler); return args.handler; }
          if (command === 'plugin:event|unlisten') return;
          if (command === 'plugin:dialog|open') return 'D:/Galgame';
          if (command === 'list_entries') return structuredClone(window.archives);
          if (command === 'load_steam_scan_results') return null;
          if (command === 'load_galgame_scan_results') { if (window.corrupt) throw 'galgame-cache-corrupt'; return structuredClone(window.cache); }
          if (command === 'scan_galgame_saves') {
            window.callbacks.get(window.events.get('galgame-scan-progress'))({ payload: { taskId: args.taskId, checkedDirectories: 25, foundGames: 3 } });
            if (window.slowScan) return new Promise((resolve, reject) => { window.cancelPending = reject; });
            window.cache = { rootPath: args.rootPath, scannedAt: 1789450000, warnings: [], games: [
              { name: '中文游戏', installPath: 'D:/Galgame/合集/中文游戏', engine: 'Kirikiri', sources: [{ path: 'D:/Galgame/合集/中文游戏/savedata', kind: 'folder', evidence: 'engine' }, { path: 'C:/Users/Test/AppData/Roaming/厂商/中文游戏', kind: 'folder', evidence: 'name' }] },
              { name: '日本語ゲーム', installPath: 'D:/Galgame/合集/更深/日本語ゲーム', engine: 'RPG Maker VX Ace', sources: [{ path: 'D:/Galgame/合集/更深/日本語ゲーム/Save01.rvdata2', kind: 'file', evidence: 'engine' }] },
              { name: '尚未保存', installPath: 'D:/Galgame/New', engine: 'Ren’Py', sources: [] },
            ] };
            return structuredClone(window.cache);
          }
          if (command === 'cancel_galgame_scan') { window.cancelPending?.('galgame-scan-cancelled'); return; }
          if (command === 'validate_galgame_sources') return window.invalid ? args.sources.map(source => source.path) : [];
          if (command === 'add_entry') {
            const archive = { id: args.name, name: args.name, sources: args.sourcePaths.map((path, index) => ({ path, kind: args.sourceKinds?.[index] || 'folder' })) };
            window.archives.push(archive); return structuredClone(archive);
          }
          if (command === 'create_snapshot') throw 'snapshot failed';
          throw `Unexpected command: ${command}`;
        },
      };
      const { createApp } = await import('/node_modules/.vite/deps/vue.js');
      const { default: Dialog } = await import('/src/components/SteamScanDialog.vue');
      window.mount = () => { window.app = createApp(Dialog, { onClose: () => window.app.unmount() }); window.app.mount('#test'); };
      window.mount();
    });
    const galTab = page.getByRole('tab', { name: 'Galgame Beta' });
    await galTab.click();
    await page.getByRole('button', { name: '选择 Galgame 合集文件夹' }).click();
    assert.equal(await page.locator('#galgame-root').inputValue(), 'D:/Galgame');
    await page.getByRole('button', { name: '开始扫描', exact: true }).click();
    await page.getByText('识别 3 个游戏，2 个找到存档位置。', { exact: true }).waitFor();
    assert.equal(await page.locator('.galgame-settings .source input:checked').count(), 0);
    await page.getByText('名称匹配，请核对', { exact: false }).waitFor();
    await page.getByRole('button', { name: '全选可添加项' }).click();
    assert.equal(await page.getByRole('button', { name: '添加选中的 2 个存档' }).isEnabled(), true);
    await page.evaluate(async () => { const { setLocale } = await import('/src/services/i18n.ts'); setLocale('en'); });
    await page.getByRole('button', { name: 'Add 2 selected archives' }).waitFor();
    const screenshots = fs.mkdtempSync(path.join(os.tmpdir(), 'chronicle-galgame-ui-'));
    await page.screenshot({ path: path.join(screenshots, 'english-light.png') });
    await page.evaluate(async () => { const { applyAppearance } = await import('/src/services/appearance.ts'); applyAppearance({ colorTheme: 'indigo', colorMode: 'dark' }); const { setLocale } = await import('/src/services/i18n.ts'); setLocale('zh-CN'); });
    await page.screenshot({ path: path.join(screenshots, 'chinese-dark.png') });
    assert.equal(await page.evaluate(() => document.querySelector('.galgame-settings').scrollWidth <= document.querySelector('.galgame-settings').clientWidth), true);
    await page.evaluate(() => { window.slowScan = true; });
    await page.getByRole('button', { name: '刷新扫描', exact: true }).click();
    await page.getByText('已检查 25 个目录，识别 3 个游戏。').waitFor();
    await page.getByRole('button', { name: '取消扫描', exact: true }).click();
    await page.getByText('已取消扫描，保留上次结果。').waitFor();
    await page.getByText('识别 3 个游戏，2 个找到存档位置。', { exact: true }).waitFor();
    await page.evaluate(() => { window.slowScan = false; window.invalid = true; });
    await page.getByRole('button', { name: '添加选中的 2 个存档' }).click();
    await page.getByRole('alert').filter({ hasText: '存档路径已失效' }).waitFor();
    assert.equal(await page.evaluate(() => window.archives.length), 0);
    await page.evaluate(() => { window.invalid = false; });
    await page.getByRole('checkbox', { name: '添加后立即备份' }).check();
    await page.getByRole('button', { name: '添加选中的 2 个存档' }).click();
    await page.getByRole('alert').filter({ hasText: '初始备份失败' }).waitFor();
    assert.equal(await page.evaluate(() => window.archives.length), 2);
    assert.equal(await page.getByRole('button', { name: '添加选中的 0 个存档' }).isDisabled(), true);
    await page.getByRole('button', { name: '关闭游戏存档识别' }).click();
    await page.evaluate(() => window.mount());
    await galTab.click();
    await page.getByText('识别 3 个游戏，2 个找到存档位置。', { exact: true }).waitFor();
    assert.equal(await page.locator('#galgame-root').inputValue(), 'D:/Galgame');
    assert.equal(await page.evaluate(() => window.calls.filter(call => call.command === 'scan_galgame_saves').length), 2);
    await page.getByRole('tab', { name: 'Steam', exact: true }).click();
    await page.getByRole('button', { name: '扫描游戏存档', exact: true }).waitFor();
    await page.keyboard.press('ArrowRight');
    assert.equal(await galTab.getAttribute('aria-selected'), 'true');
    await page.getByRole('button', { name: '关闭游戏存档识别' }).click();
    await page.evaluate(() => { window.corrupt = true; window.mount(); });
    await galTab.click();
    await page.getByRole('alert').filter({ hasText: '上次扫描结果无法读取' }).waitFor();
    assert.equal(await page.getByRole('button', { name: '开始扫描', exact: true }).isEnabled(), true);
    assert.deepEqual(errors, []);
    console.log(`Galgame browser checks passed. Screenshots: ${screenshots}`);
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
