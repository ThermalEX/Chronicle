// Smoke-test the packaged Windows executable in an isolated portable library.
const { chromium } = require('playwright');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const assert = require('node:assert/strict');

(async () => {
  const executable = process.argv[2];
  assert.ok(executable, 'Pass the built executable path');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'chronicle-galgame-native-'));
  const binary = path.join(root, 'Chronicle.exe');
  fs.copyFileSync(executable, binary);
  fs.writeFileSync(path.join(root, 'portable.marker'), '');
  const config = path.join(root, 'Chronicle-data', 'config');
  fs.mkdirSync(config, { recursive: true });
  fs.writeFileSync(path.join(config, 'onboarding-local.json'), JSON.stringify({ status: 'completed', seenTips: [] }));
  const collection = path.join(root, 'collection');
  const game = path.join(collection, '合集', '中文游戏');
  const save = path.join(game, 'savedata');
  fs.mkdirSync(save, { recursive: true });
  fs.writeFileSync(path.join(game, 'game.exe'), 'fixture');
  fs.writeFileSync(path.join(game, 'data.xp3'), 'fixture');
  fs.writeFileSync(path.join(save, 'save.dat'), 'unchanged');
  const port = 19333;
  const child = spawn(binary, [], { windowsHide: true, env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` } });
  let browser;
  try {
    for (let attempt = 0; attempt < 60; attempt++) {
      try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`); break; }
      catch { await new Promise(resolve => setTimeout(resolve, 500)); }
    }
    assert.ok(browser, 'WebView2 debugging endpoint did not open');
    const page = browser.contexts()[0].pages()[0];
    await page.locator('[data-tour="steam-entry"]').waitFor();
    await page.evaluate(root => localStorage.setItem('chronicle.galgame-root.v1', root), collection);
    await page.locator('[data-tour="steam-entry"]').click();
    await page.getByRole('tab', { name: 'Galgame Beta' }).click();
    await page.getByRole('button', { name: '开始扫描', exact: true }).click();
    await page.getByText('识别 1 个游戏，1 个找到存档位置。', { exact: true }).waitFor();
    assert.equal(await page.locator('.galgame-settings .source input:checked').count(), 0);
    await page.getByRole('button', { name: '全选可添加项' }).click();
    await page.getByRole('checkbox', { name: '添加后立即备份' }).uncheck();
    await page.getByRole('button', { name: '添加选中的 1 个存档' }).click();
    await page.getByRole('button', { name: '添加选中的 0 个存档' }).waitFor();
    const entries = await page.evaluate(() => window.__TAURI_INTERNALS__.invoke('list_entries'));
    assert.equal(entries.length, 1);
    assert.equal(entries[0].name, '中文游戏');
    await page.getByRole('button', { name: '关闭游戏存档识别' }).click();
    await page.locator('[data-tour="steam-entry"]').click();
    await page.getByRole('tab', { name: 'Galgame Beta' }).click();
    await page.getByText('识别 1 个游戏，1 个找到存档位置。', { exact: true }).waitFor();
    assert.ok(fs.existsSync(path.join(root, 'Chronicle-data/cache/galgame-scan-results.json')));
    assert.equal(fs.readFileSync(path.join(save, 'save.dat'), 'utf8'), 'unchanged');
    await page.screenshot({ path: path.join(root, 'native.png') });
    console.log('Native scan, import and cache smoke passed:', root);
  } finally {
    if (browser) await browser.close();
    child.kill();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
