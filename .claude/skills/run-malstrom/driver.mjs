#!/usr/bin/env node
// Minimal chromium-cli-style REPL for driving the malstrom frontend
// headlessly, built on the `@playwright/test` package already installed as
// a devDependency of this repo (chromium-cli itself is not available in
// every environment this skill runs in). Must be run with CWD at the repo
// root so this import resolves against node_modules.
//
// Reads newline-delimited commands from stdin, one per line:
//   nav <url>
//   wait-for text=<substring>          (also accepts a raw CSS selector)
//   click <selector>                   (Playwright selector, e.g. "role=button[name=/Major/]")
//   click-text <substring>             (shorthand: page.getByText(substring).click())
//   fill <selector> <text...>
//   screenshot [path]                  (default: screenshots/<n>.png, relative to CWD)
//   eval <js expression>               (runs in page context via page.evaluate)
//   console                            (print collected console/page errors so far)
//   quit
//
// Example:
//   node .claude/skills/run-malstrom/driver.mjs <<'EOF'
//   nav http://localhost:1420
//   wait-for text=Sunset Drive
//   screenshot initial.png
//   quit
//   EOF

import { chromium } from '@playwright/test';
import * as readline from 'node:readline';
import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';

const SCREENSHOT_DIR = 'screenshots';
mkdirSync(SCREENSHOT_DIR, { recursive: true });

const browser = await chromium.launch();
const page = await browser.newPage();

const errors = [];
page.on('console', (msg) => {
  if (msg.type() === 'error') errors.push(`[console] ${msg.text()}`);
});
page.on('pageerror', (err) => errors.push(`[pageerror] ${String(err)}`));

let shotCount = 0;

function parseSelectorArg(arg) {
  // `text=foo` -> page.getByText('foo'); anything else -> raw CSS/Playwright selector.
  if (arg.startsWith('text=')) return page.getByText(arg.slice('text='.length));
  return page.locator(arg);
}

async function handle(line) {
  const trimmed = line.trim();
  if (!trimmed) return;
  const [cmd, ...rest] = trimmed.split(' ');
  const arg = rest.join(' ');

  try {
    switch (cmd) {
      case 'nav': {
        await page.goto(arg, { waitUntil: 'domcontentloaded' });
        console.log(`ok: navigated to ${arg}`);
        break;
      }
      case 'wait-for': {
        await parseSelectorArg(arg).first().waitFor({ timeout: 15000 });
        console.log(`ok: found ${arg}`);
        break;
      }
      case 'click': {
        await page.locator(arg).first().click();
        console.log(`ok: clicked ${arg}`);
        break;
      }
      case 'click-text': {
        await page.getByText(arg).first().click();
        console.log(`ok: clicked text=${arg}`);
        break;
      }
      case 'fill': {
        const [selector, ...textParts] = rest;
        await page.locator(selector).fill(textParts.join(' '));
        console.log(`ok: filled ${selector}`);
        break;
      }
      case 'screenshot': {
        const path = arg || join(SCREENSHOT_DIR, `${++shotCount}.png`);
        mkdirSync(dirname(path), { recursive: true });
        await page.screenshot({ path, fullPage: true });
        console.log(`ok: screenshot -> ${path}`);
        break;
      }
      case 'eval': {
        const result = await page.evaluate(new Function(`return (${arg})`));
        console.log(`ok: ${JSON.stringify(result)}`);
        break;
      }
      case 'console': {
        console.log(errors.length ? errors.join('\n') : 'ok: no errors captured');
        break;
      }
      case 'quit': {
        await browser.close();
        process.exit(0);
        break;
      }
      default: {
        console.log(`error: unknown command "${cmd}"`);
      }
    }
  } catch (err) {
    console.log(`error: ${err instanceof Error ? err.message : String(err)}`);
  }
}

const rl = readline.createInterface({ input: process.stdin });
for await (const line of rl) {
  await handle(line);
}
await browser.close();
