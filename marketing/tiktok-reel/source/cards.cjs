const { chromium } = require('/home/user/InstantPad/node_modules/.pnpm/playwright@1.64.1/node_modules/playwright');
const path = require('path');
const DIR = __dirname;
const css = `
@font-face{font-family:L;src:url(Lexend-Regular.ttf);font-weight:400}
@font-face{font-family:L;src:url(Lexend-SemiBold.ttf);font-weight:600}
@font-face{font-family:L;src:url(Lexend-Bold.ttf);font-weight:700}
@font-face{font-family:L;src:url(Lexend-Black.ttf);font-weight:900}
*{box-sizing:border-box;margin:0}
html,body{width:1080px;height:1920px;overflow:hidden}
body{font-family:L;color:#fff;background:
 radial-gradient(900px 700px at 50% 8%, rgba(255,176,58,.20), transparent 70%),
 radial-gradient(800px 800px at 50% 100%, rgba(80,120,255,.10), transparent 70%),
 linear-gradient(#17181c,#202127 60%,#17181c);position:relative}
.grid{position:absolute;inset:0;background-image:linear-gradient(rgba(255,255,255,.035) 2px,transparent 2px),linear-gradient(90deg,rgba(255,255,255,.035) 2px,transparent 2px);background-size:72px 72px;mask-image:linear-gradient(transparent,#000 30%,#000 70%,transparent)}
.brand{position:absolute;top:150px;left:0;right:0;display:flex;gap:22px;align-items:center;justify-content:center}
.brand img{width:84px}
.word{font-weight:900;font-size:64px;letter-spacing:-1px}
.word b{color:#ffb03a;font-weight:900}
.center{position:absolute;left:70px;right:70px;top:300px;bottom:600px;display:flex;flex-direction:column;align-items:center;justify-content:center;text-align:center}
.big img{width:380px;filter:drop-shadow(0 0 60px rgba(255,176,58,.45))}
.title{font-weight:900;font-size:140px;letter-spacing:-3px;margin-top:30px}
.title b{color:#ffb03a}
.tag{font-size:46px;font-weight:600;color:#c9cbd3;margin-top:18px}
.node{background:#2b2d34;border:3px solid #474a55;border-radius:28px;padding:26px 36px;font-size:46px;font-weight:700;box-shadow:0 10px 30px rgba(0,0,0,.4)}
.node.hot{border-color:#ffb03a;color:#ffb03a}
.node.game{border-color:#5ef08a;color:#5ef08a}
.row{display:flex;gap:30px;justify-content:center}
.arrow{font-size:70px;color:#5ef08a;line-height:1;margin:18px 0;font-weight:900}
.pill{display:inline-block;background:#ffb03a;color:#17181c;font-weight:900;font-size:76px;border-radius:30px;padding:18px 44px;margin:16px 0;transform:rotate(-2deg)}
.feat{font-size:70px;font-weight:900;margin:16px 0}
.feat.dim{color:#c9cbd3;font-size:56px;font-weight:700}
.os{display:flex;gap:22px;margin-top:22px}
.os span{background:#2b2d34;border:3px solid #474a55;border-radius:22px;padding:18px 28px;font-size:46px;font-weight:700}
.url{font-size:44px;font-weight:700;color:#ffb03a;margin-top:26px}
.cta{font-weight:900;font-size:100px;letter-spacing:-2px;margin-top:40px}
.hide{visibility:hidden}
`;
const brand = `<div class="brand"><img src="logo.svg"><div class="word">Instant<b>Pad</b></div></div>`;
const page = (inner, withBrand = true) => `<!doctype html><html><head><meta charset=utf-8><style>${css}</style></head><body><div class=grid></div>${withBrand ? brand : ''}${inner}</body></html>`;
const diagram = (s) => page(`<div class=center>
  <div class=row><div class="node ${s<2?'hide':''}">🎙 Sua voz</div><div class="node">🔊 Os sons</div></div>
  <div class=arrow><svg width="60" height="70" viewBox="0 0 60 70"><path d="M30 4 V54 M10 36 L30 60 L50 36" stroke="#5ef08a" stroke-width="9" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg></div>
  <div class="node hot" style="font-size:58px;padding:30px 50px">InstantPad</div>
  <div class="arrow ${s<3?'hide':''}"><svg width="60" height="70" viewBox="0 0 60 70"><path d="M30 4 V54 M10 36 L30 60 L50 36" stroke="#5ef08a" stroke-width="9" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg></div>
  <div class="node ${s<3?'hide':''}">Cabo virtual</div>
  <div class="arrow ${s<3?'hide':''}"><svg width="60" height="70" viewBox="0 0 60 70"><path d="M30 4 V54 M10 36 L30 60 L50 36" stroke="#5ef08a" stroke-width="9" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg></div>
  <div class="node game ${s<3?'hide':''}">Discord ou o jogo</div></div>`);
const feats = (s) => page(`<div class=center>
  <div class=pill>É DE GRAÇA</div>
  <div class="feat ${s<2?'hide':''}">Código aberto</div>
  <div class="os ${s<3?'hide':''}"><span>Windows</span><span>macOS</span><span>Linux</span></div></div>`);
const cta = (s) => page(`<div class=center>
  <div class=big><img src="logo.svg" style="width:${s==2?400:330}px"></div>
  <div class=title style="font-size:120px">Instant<b>Pad</b></div>
  <div class=cta>Baixe grátis</div>
  <div class=url>github.com/eljonathas/InstantPad</div>
  <div class="tag" style="margin-top:34px">Link na bio</div></div>`, false);
const intro = page(`<div class=center><div class=big><img src="logo.svg"></div><div class=title>Instant<b>Pad</b></div><div class=tag>O soundboard pro Discord e pros jogos</div></div>`, false);
const bg = page('');
const cards = { bg, intro, diagram1: diagram(1), diagram2: diagram(2), diagram3: diagram(3), feats1: feats(1), feats2: feats(2), feats3: feats(3), cta1: cta(1), cta2: cta(2) };
(async () => {
  const b = await chromium.launch({ executablePath: '/opt/pw-browsers/chromium' });
  const p = await b.newPage({ viewport: { width: 1080, height: 1920 } });
  const fs = require('fs');
  for (const [k, html] of Object.entries(cards)) {
    fs.writeFileSync(path.join(DIR, k + '.html'), html);
    await p.goto('file://' + path.join(DIR, k + '.html'));
    await p.evaluate(() => document.fonts.ready);
    await p.waitForTimeout(150);
    await p.screenshot({ path: path.join(DIR, k + '.png') });
  }
  await b.close();
})();
