// One continuous take of the motion reel: the real InstantPad interface (browser mock, pt-BR)
// inside a 1080x1920 stage with an animated camera, recorded with a CDP screencast.
const { chromium } = require('/home/user/InstantPad/node_modules/.pnpm/playwright@1.64.1/node_modules/playwright');
const fs = require('fs'); const path = require('path');
const DIR = __dirname, ASSETS = path.join(DIR, '..', 'cards'), OUT = path.join(DIR, 'take');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const smooth = (x) => x <= 0 ? 0 : x >= 1 ? 1 : x * x * (3 - 2 * x);

const K = [[0, 551, 470, 1.45], [4.4, 551, 450, 1.58], [6.6, 440, 330, 1.95], [9.9, 440, 330, 1.95], [11.0, 551, 450, 1.5],
  [12.95, 660, 400, 0.80], [14.9, 660, 400, 0.86], [15.7, 1097, 300, 2.3], [17.0, 1097, 220, 2.4], [18.2, 1097, 240, 2.3],
  [18.75, 760, 380, 1.15], [19.6, 700, 400, 1.15], [20.1, 492, 400, 2.3], [21.6, 492, 400, 2.4],
  [25.9, 437, 240, 2.1], [26.1, 437, 240, 2.1], [26.9, 494, 240, 2.6], [27.6, 494, 240, 2.6], [28.35, 381, 240, 2.6], [29.6, 381, 240, 2.7]];
const OV = [['intro', 11.0, 12.95], ['diagram', 21.6, 26.1], ['feats', 29.6, 35.1], ['cta', 35.1, 99]];
const PUNCH = [[7.01, .07], [8.06, .07], [9.07, .06], [20.2, .06], [27.06, .05], [28.55, .05]];
const SLAM = 39.45;

const CURSOR = `(() => {
  const st = document.createElement('style'); st.textContent = '*{cursor:none !important}'; document.head.append(st);
  const c = document.createElement('div'); c.id = '__cursor';
  c.innerHTML = '<svg width="30" height="30" viewBox="0 0 24 24"><path d="M4 2 L4 19 L8.5 14.8 L11.6 21.5 L14.4 20.3 L11.3 13.7 L17.5 13.7 Z" fill="#fff" stroke="#111" stroke-width="1.4" stroke-linejoin="round"/></svg>';
  Object.assign(c.style, {position:'fixed', left:'-100px', top:'-100px', zIndex:2147483647, pointerEvents:'none', transition:'transform 90ms ease-out', transformOrigin:'4px 2px', filter:'drop-shadow(0 2px 4px rgba(0,0,0,.6))'});
  const ring = document.createElement('div');
  Object.assign(ring.style, {position:'fixed', width:'44px', height:'44px', marginLeft:'-22px', marginTop:'-22px', borderRadius:'50%', border:'3px solid rgba(255,190,60,.95)', opacity:0, zIndex:2147483646, pointerEvents:'none'});
  document.documentElement.append(c, ring);
  const mv = e => { c.style.left = (e.clientX - 4) + 'px'; c.style.top = (e.clientY - 2) + 'px'; };
  addEventListener('mousemove', mv, true); addEventListener('pointermove', mv, true);
  addEventListener('mousedown', e => { c.style.transform = 'scale(.82)'; ring.style.left = e.clientX + 'px'; ring.style.top = e.clientY + 'px';
    ring.animate([{opacity:1, transform:'scale(.4)'}, {opacity:0, transform:'scale(1.6)'}], {duration:450, easing:'ease-out'}); }, true);
  addEventListener('mouseup', () => { c.style.transform = 'scale(1)'; }, true);
  window.__hideCursor = () => { c.style.left = '-100px'; };
})();`;

(async () => {
  const browser = await chromium.launch({ executablePath: '/opt/pw-browsers/chromium' });
  const ctx = await browser.newContext({ locale: 'pt-BR', viewport: { width: 1080, height: 1920 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  page.on('pageerror', (e) => console.log('pageerror', e.message));
  await page.route('**/__stage', (r) => r.fulfill({ contentType: 'text/html', body: fs.readFileSync(path.join(DIR, 'stage.html'), 'utf8') }));
  await page.route('**/__a/*', (r) => r.fulfill({ path: path.join(ASSETS, path.basename(new URL(r.request().url()).pathname)) }));
  await page.goto('http://localhost:1420/__stage');
  await page.evaluate(() => { window.__K = [[0, 660, 400, 0.8]]; });
  const frame = await (await page.$('#app')).contentFrame();
  await frame.getByRole('button', { name: '01 Vine boom', exact: true }).waitFor();
  await frame.evaluate(CURSOR);
  await sleep(600);

  const appCenter = async (loc) => loc.evaluate((el) => { const r = el.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; });
  const toPage = ([x, y]) => page.evaluate(([x, y]) => { const r = document.getElementById('cam').getBoundingClientRect(); const s = r.width / 1320; return [r.left + x * s, r.top + y * s]; }, [x, y]);
  let pos = [700, 600];
  const moveApp = async (p) => { pos = p; const [x, y] = await toPage(p); await page.mouse.move(x, y); };
  const glide = async (target, ms) => {
    const to = Array.isArray(target) ? target : await appCenter(target);
    const from = pos, t0 = Date.now();
    for (;;) { const x = smooth((Date.now() - t0) / ms); await moveApp([from[0] + (to[0] - from[0]) * x, from[1] + (to[1] - from[1]) * x]); if (x >= 1) break; await sleep(12); }
  };
  const pad = (n) => frame.getByRole('button', { name: n, exact: true });
  const row = (n) => frame.locator('.sound-row:visible', { hasText: n }).locator('.sound-name');

  // Setup, off the record: pads 04 to 08 get sounds from the trending list.
  for (const [n, slot] of [['Airhorn', '04'], ['Wilhelm scream', '05'], ['Rimshot', '06'], ['Crickets', '07'], ['Dun dun dun', '08']]) {
    pos = await appCenter(row(n)); await moveApp(pos); await page.mouse.down();
    await glide(pad(slot), 120); await page.mouse.up(); await sleep(150);
  }
  await frame.evaluate(() => window.__hideCursor()); await sleep(2500);

  // Record.
  fs.rmSync(OUT, { recursive: true, force: true }); fs.mkdirSync(OUT, { recursive: true });
  const cdp = await ctx.newCDPSession(page);
  const frames = [];
  cdp.on('Page.screencastFrame', (f) => { frames.push({ t: f.metadata.timestamp, data: f.data }); cdp.send('Page.screencastFrameAck', { sessionId: f.sessionId }).catch(() => {}); });
  await cdp.send('Page.startScreencast', { format: 'jpeg', quality: 93, maxWidth: 1080, maxHeight: 1920, everyNthFrame: 1 });
  await sleep(400);
  await page.evaluate(([K, OV, P, SL]) => { window.__K = K; window.__OV = OV; window.__PUNCH = P; window.__SLAM = SL; }, [K, OV, PUNCH, SLAM]);
  // OV/PUNCH are read once at load, so put them where the loop reads them.
  await page.evaluate(() => { OV.push(...window.__OV); PUNCH.push(...window.__PUNCH); window.__start(); });
  const t0 = Date.now() / 1000;
  const marks = {};
  const at = async (s) => { const w = t0 + s - Date.now() / 1000; if (w > 0) await sleep(w * 1000); };
  const mark = (k) => { marks[k] = Date.now() / 1000 - t0; };
  const click = async (k) => { await page.mouse.down(); mark(k); await page.mouse.up(); };

  await at(4.4); pos = [760, 640]; await moveApp(pos); await glide(pad('01 Vine boom'), 2200);
  await at(7.01); await moveApp(await appCenter(pad('01 Vine boom'))); await click('vine');
  await at(7.3); await glide(pad('04 Airhorn'), 600);
  await at(8.06); await moveApp(await appCenter(pad('04 Airhorn'))); await click('airhorn');
  await at(8.3); await glide(pad('03 Sad trombone'), 600);
  await at(9.07); await moveApp(await appCenter(pad('03 Sad trombone'))); await click('trombone');
  await at(9.4); await glide([560, 600], 1000);

  const input = frame.locator('.right-panel input').first();
  await at(16.5); await glide(input, 650);
  await at(17.25); await click('input'); await input.focus();
  await input.pressSequentially('Applause', { delay: 45 }); await input.press('Enter'); mark('searched');
  await frame.locator('.sound-row:visible', { hasText: 'Applause' }).waitFor();
  await at(18.25); await glide(row('Applause'), 420);
  await at(18.72); await moveApp(await appCenter(row('Applause'))); await page.mouse.down(); mark('grab');
  await glide(pad('09'), 850);
  await at(19.65); await moveApp(await appCenter(pad('09'))); await page.mouse.up(); mark('drop');
  await at(20.2); await moveApp(await appCenter(pad('09 Applause'))); await click('applause');

  await at(25.0);
  await frame.getByRole('tab', { name: 'Voice' }).evaluate((el) => el.click()); mark('voiceTab');
  pos = [300, 520];
  await at(26.15); await moveApp(pos); await glide(frame.locator('.effect', { hasText: 'Voz grossa' }), 750);
  await at(27.06); await moveApp(await appCenter(frame.locator('.effect', { hasText: 'Voz grossa' }))); await click('deep');
  await at(27.65); await glide(frame.locator('.effect', { hasText: 'Voz fina' }), 700);
  await at(28.55); await moveApp(await appCenter(frame.locator('.effect', { hasText: 'Voz fina' }))); await click('thin');
  await at(41.3);
  await cdp.send('Page.stopScreencast');
  frames.forEach((f, i) => fs.writeFileSync(`${OUT}/${String(i).padStart(5, '0')}.jpg`, Buffer.from(f.data, 'base64')));
  fs.writeFileSync(`${OUT}/frames.json`, JSON.stringify({ times: frames.map((f) => f.t - t0), marks }));
  console.log(frames.length, 'frames', JSON.stringify(marks));
  await browser.close();
})();
