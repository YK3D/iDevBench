// Injected into the app by record.cjs for the showreel's screen-mirroring and AI-agent takes.
//
// There is no iPhone in the recording machine: the app's Simulator accepts mirroring and every touch
// (and ignores them) but has no picture. So this draws a stand-in iPhone screen where the app shows
// the mirrored picture (the canvas, at the real 1170x2532 size), a terminal with an AI agent (styled
// like Claude Code) that calls the app's real MCP tools, and the agent's pointer on the phone. The
// recorder drives all of it one video frame at a time; nothing here keeps its own time, except CSS
// animations and transitions, which the recorder's clock steps.
//
// Phone positions are in iPhone points (390 x 844), the same fractions the MCP tools take.
(() => {
  const PW = 390, PH = 844;
  const COLS = [62, 151.3, 238.7, 328];
  const ROW0 = 74, ROW = 96, ICON = 62;
  const slot = (i) => ({ x: COLS[i % 4], y: ROW0 + Math.floor(i / 4) * ROW + ICON / 2 });

  // ---- app icons: a gradient tile and a simple glyph (stroked unless marked fill)
  const G = {
    phone: ['<path fill="#fff" stroke="none" d="M20 15.9v3a2 2 0 0 1-2.2 2 18 18 0 0 1-7.8-2.8 17.7 17.7 0 0 1-5.4-5.4A18 18 0 0 1 1.8 4.9 2 2 0 0 1 3.8 2.7h3a2 2 0 0 1 2 1.7c.1.9.3 1.7.6 2.5a2 2 0 0 1-.4 2.1L7.7 10.3a14.4 14.4 0 0 0 5.4 5.4l1.3-1.3a2 2 0 0 1 2.1-.4c.8.3 1.6.5 2.5.6a2 2 0 0 1 1.7 2z"/>', "#5be37d", "#21b14a"],
    safari: ['<circle cx="12" cy="12" r="9.5"/><path fill="#fff" stroke="none" d="m16.3 7.7-2.6 6-6 2.6 2.6-6z"/>', "#62c6ff", "#1e6ff0"],
    messages: ['<path fill="#fff" stroke="none" d="M21 11.3c0 4.6-4 8.2-9 8.2-1.3 0-2.6-.3-3.7-.7L3.5 20.5l1.5-3.8A7.8 7.8 0 0 1 3 11.3C3 6.7 7 3 12 3s9 3.7 9 8.3z"/>', "#5be37d", "#20b247"],
    camera: ['<path d="M14.5 4.5h-5L7.6 7H4.5A1.8 1.8 0 0 0 2.7 8.8v9a1.8 1.8 0 0 0 1.8 1.8h15a1.8 1.8 0 0 0 1.8-1.8v-9A1.8 1.8 0 0 0 19.5 7h-3.1z"/><circle cx="12" cy="13" r="3.6"/>', "#9a9aa1", "#4a4a50"],
    mail: ['<rect x="2.5" y="5" width="19" height="14" rx="2.2"/><path d="m3 6.5 9 6.5 9-6.5"/>', "#5fb8ff", "#1170ff"],
    facetime: ['<rect x="2.5" y="6.5" width="13" height="11" rx="2.5" fill="#fff" stroke="none"/><path fill="#fff" stroke="none" d="m21.5 7.5-5 3.6v1.8l5 3.6z"/>', "#5be37d", "#1fae46"],
    contacts: ['<circle cx="12" cy="9" r="3.6"/><path d="M5.5 19.5a6.5 6.5 0 0 1 13 0"/>', "#b7b7bd", "#76767c"],
    maps: ['<path fill="#fff" stroke="none" d="M12 21.6s-6.8-6.2-6.8-11.4a6.8 6.8 0 0 1 13.6 0c0 5.2-6.8 11.4-6.8 11.4z"/><circle cx="12" cy="10.2" r="2.6" fill="#2a9d8f" stroke="none"/>', "#7ad989", "#2a9d8f"],
    findmy: ['<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="5.2"/><circle cx="12" cy="12" r="1.8" fill="#fff" stroke="none"/>', "#59de80", "#16a34a"],
    weather: ['<circle cx="9" cy="8.5" r="3.4" fill="#ffd54a" stroke="none"/><path fill="#fff" stroke="none" d="M17.3 19.5H8.2a4.3 4.3 0 1 1 1.3-8.4 5.3 5.3 0 0 1 9.7 1.9 3.3 3.3 0 0 1-1.9 6.5z"/>', "#57b0ff", "#1d5fd8"],
    wallet: ['<rect x="3.5" y="5" width="17" height="14" rx="2.5"/><path d="M3.5 9.5h17M3.5 13.5h17"/>', "#3a3a3e", "#111113"],
    translate: ['<path d="M4 6.5h9M8.5 4.5v2M11 6.5c-.6 3.4-2.8 6-6 7.5M6.5 9.5c1 2 2.6 3.4 4.5 4.3M13.5 20l3.5-8.5 3.5 8.5M14.7 17.3h4.6"/>', "#3d8bff", "#0b3fc4"],
    photos: ["photos", "#ffffff", "#f1f1f4"],
    music: ['<path d="M9.5 17.5V6l10-2v11.5"/><circle cx="7" cy="17.5" r="2.6" fill="#fff" stroke="none"/><circle cx="17" cy="15.5" r="2.6" fill="#fff" stroke="none"/>', "#ff6b86", "#f2193b"],
    podcasts: ['<circle cx="12" cy="10.5" r="2.4" fill="#fff" stroke="none"/><path d="M12 13.5v7M8.2 14.6a5.4 5.4 0 1 1 7.6 0M5.6 17.3a9 9 0 1 1 12.8 0"/>', "#d77bff", "#8a2be2"],
    books: ['<path d="M3 5h5.5A3.5 3.5 0 0 1 12 8.5V20a2.6 2.6 0 0 0-2.6-2.6H3zM21 5h-5.5A3.5 3.5 0 0 0 12 8.5V20a2.6 2.6 0 0 1 2.6-2.6H21z"/>', "#ffad33", "#ff6a00"],
    news: ['<path stroke-width="2.6" d="M6 19V5l12 14V5"/>', "#ff6680", "#ea1d4c"],
    tv: ['<rect x="2.5" y="6.5" width="19" height="12.5" rx="2"/><path d="m16.5 3-4.5 3.5L7.5 3"/>', "#3a3a3e", "#111113"],
    notes: ["notes", "#ffffff", "#f4f4f6"],
    calendar: ["calendar", "#ffffff", "#f4f4f6"],
    reminders: ["reminders", "#ffffff", "#f4f4f6"],
    files: ['<path fill="#fff" stroke="none" d="M3 7.5A2 2 0 0 1 5 5.5h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>', "#5cb2ff", "#1170ff"],
    clock: ['<circle cx="12" cy="12" r="9" fill="#fff" stroke="none"/><path stroke="#111" d="M12 6.8V12l3.4 2.2"/>', "#3a3a3e", "#111113"],
    calculator: ['<path d="M7 7.5h10"/><circle cx="7.5" cy="12" r=".9" fill="#fff"/><circle cx="12" cy="12" r=".9" fill="#fff"/><circle cx="16.5" cy="12" r=".9" fill="#ff9f0a" stroke="#ff9f0a"/><circle cx="7.5" cy="16.5" r=".9" fill="#fff"/><circle cx="12" cy="16.5" r=".9" fill="#fff"/><circle cx="16.5" cy="16.5" r=".9" fill="#ff9f0a" stroke="#ff9f0a"/>', "#3a3a3e", "#111113"],
    stocks: ['<path stroke="#3ddc84" d="m3.5 16.5 5-5 3.5 3.5 8-8"/><path stroke="#3ddc84" d="M15 7h5v5"/>', "#3a3a3e", "#111113"],
    voicememos: ['<path stroke="#ff453a" d="M4 10.5v3M7.5 8v8M11 5v14M14.5 8.5v7M18 10v4M21 11.2v1.6"/>', "#3a3a3e", "#111113"],
    settings: ['<circle cx="12" cy="12" r="3"/><circle cx="12" cy="12" r="6.6"/><path stroke-width="2.4" d="M12 2.6v2.2M12 19.2v2.2M2.6 12h2.2M19.2 12h2.2M5.4 5.4 7 7M17 17l1.6 1.6M5.4 18.6 7 17M17 7l1.6-1.6"/>', "#a8a8ae", "#636369"],
    health: ['<path fill="#ff375f" stroke="none" d="M12 20.3 4.8 13.4A4.6 4.6 0 0 1 12 7.4a4.6 4.6 0 0 1 7.2 6z"/>', "#ffffff", "#f4f4f6"],
  };
  const special = {
    photos: () => {
      const cols = ["#ff9f0a", "#ffd60a", "#30d158", "#40c8e0", "#0a84ff", "#bf5af2", "#ff375f", "#ff6b2c"];
      return cols.map((c, i) => `<ellipse cx="12" cy="7" rx="2.6" ry="4.6" fill="${c}" opacity=".88" transform="rotate(${i * 45} 12 12)"/>`).join("");
    },
    notes: () => '<rect x="0" y="0" width="24" height="6.5" fill="#ffd60a"/><path stroke="#c7c7cc" stroke-width="1" d="M4 11h16M4 15h16M4 19h16"/>',
    calendar: () => '<text stroke="none" x="12" y="7.6" text-anchor="middle" font-size="4.6" font-weight="700" fill="#ff3b30" font-family="Inter">THU</text><text stroke="none" x="12" y="19.6" text-anchor="middle" font-size="12.5" font-weight="400" fill="#111" font-family="Inter">26</text>',
    reminders: () => '<circle cx="6" cy="7.5" r="1.9" fill="#0a84ff"/><circle cx="6" cy="12.5" r="1.9" fill="#ff9f0a"/><circle cx="6" cy="17.5" r="1.9" fill="#ff375f"/><path stroke="#c7c7cc" stroke-width="1.2" d="M10 7.5h9M10 12.5h9M10 17.5h9"/>',
  };
  const NAMES = { phone: "Phone", safari: "Safari", messages: "Messages", camera: "Camera", mail: "Mail", facetime: "FaceTime",
    contacts: "Contacts", maps: "Maps", findmy: "Find My", weather: "Weather", wallet: "Wallet", translate: "Translate",
    photos: "Photos", music: "Music", podcasts: "Podcasts", books: "Books", news: "News", tv: "TV", notes: "Notes",
    calendar: "Calendar", reminders: "Reminders", files: "Files", clock: "Clock", calculator: "Calculator", stocks: "Stocks",
    voicememos: "Voice Memos", settings: "Settings", health: "Health" };
  const GRID = ["mail", "calendar", "photos", "maps", "notes", "weather", "music", "clock", "facetime", "findmy", "files", "podcasts",
    "settings", "books", "wallet", "reminders", "calculator", "news", "contacts", "translate", "health", "stocks", "tv", "voicememos"];
  const DOCK = ["phone", "safari", "messages", "camera"];

  function tileHTML(app) {
    const [glyph, a, b] = G[app];
    const body = special[glyph] ? special[glyph]() : glyph;
    return `<div class="dp-tile" style="background:linear-gradient(180deg,${a},${b})"><svg viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round">${body}</svg></div>`;
  }
  const folderHTML = (apps) => `<div class="dp-tile dp-folder"><div class="dp-mini">${apps.slice(0, 9).map((a) => {
    const [, c1, c2] = G[a];
    return `<i style="background:linear-gradient(180deg,${c1},${c2})"></i>`;
  }).join("")}</div></div>`;

  const SPARK = '<svg viewBox="0 0 24 24" class="spark"><path fill="currentColor" d="M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z"/></svg>';

  const css = `
  #dp { position: absolute; inset: 0; border-radius: 22px; overflow: hidden; line-height: normal; z-index: 2; pointer-events: none;
    opacity: 0; transition: opacity .38s cubic-bezier(.2,.8,.2,1); background: #000; }
  #dp.on { opacity: 1; }
  #dp-in { position: absolute; left: 0; top: 0; width: ${PW}px; height: ${PH}px; transform-origin: 0 0; font-family: Inter, system-ui, sans-serif;
    -webkit-font-smoothing: antialiased; color: #fff; }
  #dp-home { position: absolute; inset: 0; transition: transform .42s cubic-bezier(.2,.8,.2,1), filter .42s cubic-bezier(.2,.8,.2,1);
    background:
      radial-gradient(60% 45% at 85% 88%, rgba(255,140,80,.55), transparent 70%),
      radial-gradient(70% 50% at 10% 70%, rgba(120,90,255,.45), transparent 70%),
      radial-gradient(60% 40% at 70% 15%, rgba(40,190,170,.40), transparent 70%),
      linear-gradient(170deg, #1b1840, #0d0f1f 60%, #1a0f1a); }
  #dp-home.behind { transform: scale(1.08); filter: brightness(.55) blur(2px); }
  .dp-status { position: absolute; left: 0; right: 0; top: 0; height: 54px; display: flex; align-items: center; justify-content: space-between;
    padding: 4px 30px 0 46px; font: 600 16px/1 Inter, system-ui, sans-serif; letter-spacing: -.01em; z-index: 500; color: #fff; transition: color .3s; }
  .dp-status.dark { color: #111; }
  .dp-island { position: absolute; left: 50%; top: 11px; width: 124px; height: 36px; margin-left: -62px; border-radius: 20px; background: #000; z-index: 600; }
  .dp-sicons { display: flex; align-items: center; gap: 6px; }
  .dp-sicons svg { height: 12px; fill: currentColor; }
  .dp-icon { position: absolute; left: 0; top: 0; width: ${ICON}px; margin: -${ICON / 2}px 0 0 -${ICON / 2}px;
    transition: transform .42s cubic-bezier(.32,.72,0,1), opacity .25s; will-change: transform; }
  .dp-icon.dragging { transition: none; z-index: 4; }
  .dp-icon.gone { opacity: 0; }
  .dp-wig { transform-origin: 50% 40%; }
  .jiggle .dp-wig { animation: dp-jig .27s ease-in-out infinite alternate; }
  @keyframes dp-jig { from { transform: rotate(-1.8deg) translateY(.4px); } to { transform: rotate(1.8deg) translateY(-.4px); } }
  .dp-lift { transition: transform .24s cubic-bezier(.2,.8,.2,1); }
  .dragging .dp-lift { transform: scale(1.14); }
  .dp-tile { width: ${ICON}px; height: ${ICON}px; border-radius: 14.5px; display: grid; place-items: center; position: relative;
    box-shadow: 0 1px 2px rgba(0,0,0,.25); transition: transform .38s cubic-bezier(.3,1.6,.5,1); }
  .dp-tile svg { width: 34px; height: 34px; }
  .dp-tile.pop { transform: scale(1.16); transition: transform .16s cubic-bezier(.2,.8,.2,1); }
  .dp-folder { background: rgba(255,255,255,.26); backdrop-filter: blur(14px); }
  .dp-mini { display: grid; grid-template-columns: repeat(3, 13px); grid-auto-rows: 13px; gap: 3.5px; width: 46px; height: 46px; align-content: start; }
  .dp-mini i { border-radius: 3.5px; }
  .dp-label { margin-top: 6px; text-align: center; font-size: 11.5px; font-weight: 500; letter-spacing: -.005em; white-space: nowrap;
    text-shadow: 0 1px 3px rgba(0,0,0,.35); margin-left: -20px; margin-right: -20px; transition: opacity .2s; }
  .dp-badge { position: absolute; left: -6px; top: -6px; width: 20px; height: 20px; border-radius: 50%; background: rgba(90,90,96,.92);
    opacity: 0; transform: scale(.6); transition: opacity .22s, transform .22s; z-index: 2; }
  .dp-badge::after { content: ""; position: absolute; left: 6px; right: 6px; top: 9px; height: 2px; border-radius: 1px; background: #fff; }
  .jiggle .dp-badge { opacity: 1; transform: none; }
  .dp-search { position: absolute; left: 50%; top: 688px; transform: translateX(-50%); padding: 7px 14px; border-radius: 16px;
    background: rgba(255,255,255,.2); backdrop-filter: blur(16px); font-size: 13px; font-weight: 500; display: flex; gap: 6px; align-items: center; }
  .dp-dock { position: absolute; left: 12px; right: 12px; top: 740px; height: 90px; border-radius: 34px; background: rgba(255,255,255,.18);
    backdrop-filter: blur(22px); }
  .dp-done { position: absolute; right: 18px; top: 12px; z-index: 7; padding: 6px 15px; border-radius: 15px; background: rgba(255,255,255,.26);
    backdrop-filter: blur(16px); font-size: 14px; font-weight: 600; opacity: 0; transform: scale(.8); transition: opacity .22s, transform .22s; }
  .jiggle .dp-done { opacity: 1; transform: none; }
  .jiggle .dp-search, .jiggle .dp-status .dp-sicons { opacity: 0; transition: opacity .2s; }
  #dp-app { position: absolute; inset: 0; background: #f2f1ec; overflow: hidden; opacity: 0; border-radius: 48px;
    transform-origin: var(--ox) var(--oy); transform: scale(.16); z-index: 3;
    transition: transform .46s cubic-bezier(.2,.85,.2,1), opacity .2s, border-radius .46s cubic-bezier(.2,.85,.2,1); }
  #dp-app.open { opacity: 1; transform: none; border-radius: 0; }
  #dp-map { position: absolute; left: 0; top: 0; width: ${PW}px; height: 600px; background: #ece9e1; }
  .dp-me { position: absolute; left: 195px; top: 300px; width: 0; height: 0; z-index: 450; }
  .dp-me .halo { position: absolute; left: -45px; top: -45px; width: 90px; height: 90px; border-radius: 50%; background: rgba(48,140,255,.22);
    animation: dp-halo 2s ease-out infinite; }
  @keyframes dp-halo { from { transform: scale(.35); opacity: 1; } to { transform: scale(1); opacity: 0; } }
  .dp-me .pin { position: absolute; left: -21px; top: -21px; width: 42px; height: 42px; border-radius: 50%; background: linear-gradient(180deg,#4aa3ff,#1a6dff);
    border: 3px solid #fff; box-shadow: 0 3px 10px rgba(0,0,0,.3); display: grid; place-items: center; }
  .dp-me .pin svg { width: 18px; height: 18px; }
  .dp-sheet { position: absolute; left: 0; right: 0; top: 560px; bottom: 0; background: #fff; border-radius: 22px 22px 0 0; color: #111;
    box-shadow: 0 -4px 20px rgba(0,0,0,.12); padding: 8px 20px 0; z-index: 460; }
  .dp-handle { width: 38px; height: 5px; border-radius: 3px; background: #d1d1d6; margin: 0 auto 12px; }
  .dp-sheet h4 { margin: 0 0 12px; font-size: 24px; font-weight: 700; letter-spacing: -.02em; }
  .dp-row { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-top: 1px solid #ececef; }
  .dp-row .av { width: 44px; height: 44px; border-radius: 50%; background: #e9e9ee; display: grid; place-items: center; }
  .dp-row .av svg { width: 22px; height: 22px; }
  .dp-row b { display: block; font-size: 16px; font-weight: 600; }
  .dp-row span { font-size: 13.5px; color: #8a8a8e; }
  .dp-row em { margin-left: auto; font-style: normal; font-size: 13.5px; color: #8a8a8e; }
  .dp-tabs { position: absolute; left: 0; right: 0; bottom: 0; height: 84px; display: flex; justify-content: space-around; padding-top: 10px;
    border-top: 1px solid #ececef; background: rgba(250,250,250,.96); z-index: 470; }
  .dp-tabs div { display: flex; flex-direction: column; align-items: center; gap: 4px; font-size: 10.5px; font-weight: 500; color: #9a9a9e; }
  .dp-tabs div.on { color: #1a6dff; }
  .dp-tabs i { width: 24px; height: 20px; border-radius: 6px; background: currentColor; opacity: .85; }
  .dp-tabs div:nth-child(1) i { border-radius: 50%; width: 20px; }
  .dp-homebar { position: absolute; left: 50%; bottom: 8px; width: 134px; height: 5px; margin-left: -67px; border-radius: 3px; background: rgba(255,255,255,.85); z-index: 700; transition: background .3s; }
  .dp-homebar.dark { background: rgba(0,0,0,.8); }
  #dp-cursor { position: absolute; left: 0; top: 0; width: 0; height: 0; z-index: 800; opacity: 0; transition: opacity .22s; }
  #dp-cursor.show { opacity: 1; }
  #dp-cursor .dot { position: absolute; left: -15px; top: -15px; width: 30px; height: 30px; border-radius: 50%; background: rgba(217,119,87,.94);
    border: 2.5px solid #fff; box-shadow: 0 0 0 7px rgba(217,119,87,.22), 0 6px 16px rgba(0,0,0,.35);
    transition: transform .42s cubic-bezier(.3,1.75,.55,1); }
  #dp-cursor.down .dot { transform: scale(.74); transition: transform .12s cubic-bezier(.2,.8,.2,1); }
  #dp-cursor .tag { position: absolute; left: 16px; top: 14px; display: flex; align-items: center; gap: 5px; padding: 4px 9px 4px 6px; border-radius: 11px;
    background: #d97757; color: #fff; font: 600 12.5px/1 Inter, sans-serif; box-shadow: 0 4px 12px rgba(0,0,0,.3); white-space: nowrap; }
  #dp-cursor .tag .spark { width: 13px; height: 13px; }
  .dp-ripple { position: absolute; left: -15px; top: -15px; width: 30px; height: 30px; border-radius: 50%; border: 2.5px solid #d97757;
    animation: dp-ripple .5s cubic-bezier(.2,.8,.2,1) forwards; }
  @keyframes dp-ripple { from { transform: scale(.7); opacity: .9; } to { transform: scale(2.4); opacity: 0; } }
  .dp-hold { position: absolute; left: -24px; top: -24px; width: 48px; height: 48px; }
  .dp-hold circle { fill: none; stroke: #fff; stroke-width: 3; stroke-linecap: round; stroke-dasharray: 138; stroke-dashoffset: 138;
    transform: rotate(-90deg); transform-origin: 50% 50%; animation: dp-hold .8s linear forwards; }
  @keyframes dp-hold { to { stroke-dashoffset: 0; } }

  /* ---- the agent's terminal */
  #demo-term { position: fixed; left: 22px; top: 34px; width: 540px; height: 732px; z-index: 9000; display: flex; flex-direction: column;
    background: #1d1c1a; border: 1px solid rgba(255,255,255,.09); border-radius: 12px; box-shadow: 0 30px 80px -20px rgba(0,0,0,.75), 0 0 0 1px rgba(0,0,0,.4);
    font: 400 11.6px/1.6 "JBMono", ui-monospace, monospace; color: #e9e6e1; overflow: hidden;
    opacity: 0; transform: translateY(14px) scale(.97); transform-origin: 30% 80%;
    transition: opacity .28s cubic-bezier(.2,.8,.2,1), transform .34s cubic-bezier(.2,.8,.2,1); }
  #demo-term.open { opacity: 1; transform: none; }
  .dt-bar { height: 34px; flex: none; display: flex; align-items: center; gap: 8px; padding: 0 12px; background: #262522; border-bottom: 1px solid rgba(255,255,255,.06);
    font: 500 12px/1 Inter, sans-serif; color: #cfcac2; }
  .dt-bar .spark { width: 14px; height: 14px; color: #d97757; }
  .dt-bar .path { color: #8c877f; }
  .dt-bar .win { margin-left: auto; display: flex; gap: 18px; color: #8c877f; font-size: 13px; }
  .dt-body { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 12px 16px 10px; }
  .dt-welcome { flex: none; border: 1px solid #d97757; border-radius: 8px; padding: 8px 12px; margin-bottom: 12px; }
  .dt-welcome b { color: #fff; font-weight: 600; }
  .dt-welcome .spark { width: 12px; height: 12px; color: #d97757; vertical-align: -1px; margin-right: 6px; }
  .dt-dim { color: #8c877f; }
  .dt-ok { color: #7dd39a; }
  .dt-log { flex: 1; min-height: 0; overflow: hidden; display: flex; flex-direction: column; justify-content: flex-end; }
  .dt-line { position: relative; padding-left: 18px; white-space: pre-wrap; word-break: break-word; margin-top: 6px; }
  .dt-line.dt-res { margin-top: 0; padding-left: 40px; color: #a9a49c; }
  .dt-line.dt-res::before { content: ""; position: absolute; left: 22px; top: 2px; width: 7px; height: 8px; border-left: 1.5px solid #6d6862; border-bottom: 1.5px solid #6d6862; }
  .dt-line.dt-user { color: #a9a49c; background: rgba(255,255,255,.045); border-radius: 4px; padding: 3px 8px 3px 26px; margin-top: 8px; }
  .dt-line.dt-user::before { content: ">"; position: absolute; left: 10px; color: #8c877f; }
  .dt-line .b { position: absolute; left: 3px; top: 6px; width: 7px; height: 7px; border-radius: 50%; background: #e9e6e1; }
  .dt-line.dt-tool .b { background: #7dd39a; }
  .dt-line.dt-tool strong { font-weight: 600; color: #fff; }
  .dt-line .args { color: #a9a49c; }
  .dt-think { flex: none; height: 22px; margin-top: 8px; color: #d97757; display: flex; align-items: center; gap: 8px; opacity: 0; }
  .dt-think.on { opacity: 1; }
  .dt-think .spark { width: 13px; height: 13px; animation: dt-spin 1.6s linear infinite; }
  @keyframes dt-spin { to { transform: rotate(360deg); } }
  .dt-think .dt-dim { margin-left: 2px; }
  .dt-input { flex: none; margin-top: 8px; border: 1px solid #55514b; border-radius: 8px; padding: 7px 12px 7px 26px; position: relative; min-height: 20px;
    white-space: pre-wrap; word-break: break-word; }
  .dt-input::before { content: ">"; position: absolute; left: 11px; color: #e9e6e1; }
  .dt-caret { display: inline-block; width: 7px; height: 14px; vertical-align: -2px; background: #e9e6e1; animation: dt-blink 1.05s steps(1) infinite; }
  @keyframes dt-blink { 50% { opacity: 0; } }
  .dt-input.typing .dt-caret { animation: none; }
  .dt-foot { flex: none; display: flex; justify-content: space-between; padding: 6px 4px 0; font-size: 10.8px; }
  `;

  const status = (dark) => `<div class="dp-status${dark ? " dark" : ""}"><span>9:41</span><span class="dp-sicons">
    <svg viewBox="0 0 18 12"><rect x="0" y="8" width="3" height="4" rx="1"/><rect x="5" y="5.5" width="3" height="6.5" rx="1"/><rect x="10" y="3" width="3" height="9" rx="1"/><rect x="15" y="0" width="3" height="12" rx="1"/></svg>
    <svg viewBox="0 0 16 12"><path d="M8 2.5c2.2 0 4.2.8 5.7 2.2l1.2-1.3A10 10 0 0 0 8 .7 10 10 0 0 0 1.1 3.4l1.2 1.3A8.2 8.2 0 0 1 8 2.5zm0 3.4c1.3 0 2.5.5 3.4 1.3l1.2-1.3A6.5 6.5 0 0 0 8 4.1 6.5 6.5 0 0 0 3.4 5.9l1.2 1.3c.9-.8 2.1-1.3 3.4-1.3zm0 3.4c.5 0 .9.2 1.2.5L8 11.2 6.8 9.8c.3-.3.7-.5 1.2-.5z"/></svg>
    <svg viewBox="0 0 27 12"><rect x=".5" y=".5" width="22" height="11" rx="3.2" fill="none" stroke="currentColor" opacity=".45"/><rect x="2" y="2" width="17" height="8" rx="2"/><rect x="24" y="4" width="2" height="4" rx="1" opacity=".45"/></svg>
  </span></div>`;

  // ------------------------------------------------------------------ build
  const D = { items: [], el: {}, k: 1, map: null, glReady: () => true, follow: null, apps: {} };
  window.__demo = D;

  D.install = () => {
    const st = document.createElement("style");
    st.textContent = css;
    document.head.append(st);
    const screen = document.getElementById("mirror-screen");
    const root = document.createElement("div");
    root.id = "dp";
    root.innerHTML = `<div id="dp-in">
      <div id="dp-home">${status(false)}
        <div class="dp-done">Done</div>
        <div id="dp-grid"></div>
        <div class="dp-search"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="3" stroke-linecap="round"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m20 20-4.5-4.5"/></svg>Search</div>
        <div class="dp-dock"></div>
      </div>
      <div id="dp-app" style="--ox:151px;--oy:297px">
        <div class="dp-me"><div class="halo"></div><div class="pin"><svg viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round"><rect x="7" y="2.5" width="10" height="19" rx="2.5"/><path d="M11 18.5h2"/></svg></div></div>
        ${status(true)}
        <div class="dp-sheet"><div class="dp-handle"></div><h4>Devices</h4>
          <div class="dp-row"><div class="av"><svg viewBox="0 0 24 24" fill="none" stroke="#555" stroke-width="2" stroke-linecap="round"><rect x="7" y="2.5" width="10" height="19" rx="2.5"/><path d="M11 18.5h2"/></svg></div>
            <div><b>iPhone</b><span>This iPhone · Westminster, London</span></div><em>Now</em></div>
        </div>
        <div class="dp-tabs"><div><i></i>People</div><div class="on"><i></i>Devices</div><div><i></i>Items</div><div><i></i>Me</div></div>
      </div>
      <div class="dp-island"></div>
      <div class="dp-homebar"></div>
      <div id="dp-cursor"><div class="dot"></div><div class="tag">${SPARK}Claude</div></div>
    </div>`;
    screen.append(root);
    const grid = root.querySelector("#dp-grid");
    const make = (app, i, dock) => {
      const el = document.createElement("div");
      el.className = "dp-icon";
      el.innerHTML = `<div class="dp-wig"><div class="dp-lift">${tileHTML(app)}<div class="dp-badge"></div></div><div class="dp-label">${dock ? "" : NAMES[app]}</div></div>`;
      (dock ? root.querySelector("#dp-home") : grid).append(el);
      return el;
    };
    D.items = GRID.map((app, i) => ({ id: app, apps: [app], name: NAMES[app], el: make(app, i, false) }));
    DOCK.forEach((app, i) => { const el = make(app, i, true); el.style.transform = `translate(${COLS[i]}px, ${785}px)`; });
    D.layout(true);
    D.el = { root, inner: root.querySelector("#dp-in"), home: root.querySelector("#dp-home"), app: root.querySelector("#dp-app"),
      cursor: root.querySelector("#dp-cursor"), homebar: root.querySelector(".dp-homebar") };
    // the canvas at the phone's real size, so the app lays the picture out as it would a real one
    const c = document.getElementById("mirror-canvas");
    c.width = 1170; c.height = 2532;
    if (typeof fitMirror === "function") fitMirror();
    // now that the screen is shown (and has a size): the Find My map. A map moved into another element
    // stops drawing, so the early off-screen one only warmed the tile cache.
    if (D.map) { D.map.remove(); D.mapBox.remove(); D.map = null; }
    D.mapBox = document.createElement("div");
    D.mapBox.id = "dp-map";
    D.el.app.prepend(D.mapBox);
    D.makeMap();
    D.fit();
    new ResizeObserver(D.fit).observe(c);
  };
  D.fit = () => {
    const c = document.getElementById("mirror-canvas");
    D.k = c.clientWidth / PW;
    if (D.el.inner) D.el.inner.style.transform = `scale(${D.k})`;
  };
  D.screenOn = () => D.el.root.classList.add("on");
  D.layout = (instant) => {
    D.items.forEach((it, i) => {
      const p = slot(i);
      if (instant) it.el.style.transition = "none";
      it.el.style.transform = `translate(${p.x}px, ${p.y}px)`;
      if (instant) { void it.el.offsetWidth; it.el.style.transition = ""; }
    });
  };
  D.at = (id) => { const i = D.items.findIndex((x) => x.id === id); return i < 0 ? null : slot(i); };

  // ---- the Find My map: a light vector map that keeps the iPhone's spoofed position in the middle
  // prepareMap: a first, off-screen copy made before the recorder stops the page's clock, so the style
  // and the tiles around London download in real time and are cached for the one in the phone
  D.prepareMap = () => {
    if (D.map) return;
    D.mapBox = document.createElement("div");
    D.mapBox.id = "dp-map";
    D.mapBox.style.cssText = "position:fixed;left:-2000px;top:0";
    document.body.append(D.mapBox);
    D.makeMap();
  };
  D.makeMap = () => {
    D.map = L.map(D.mapBox, { zoomControl: false, attributionControl: false, inertia: false, fadeAnimation: false, zoomAnimation: false });
    const gl = L.maplibreGL({ style: "https://tiles.openfreemap.org/styles/liberty", attributionControl: false }).addTo(D.map);
    // tiles only: right after a move MapLibre reports "not loaded" until its next animation frame, which
    // never comes while the recorder waits with the page's clock stopped
    D.glReady = () => { const m = gl.getMaplibreMap && gl.getMaplibreMap(); return !m || !m.style || m.areTilesLoaded(); };
    D.mapLoaded = () => { const m = gl.getMaplibreMap && gl.getMaplibreMap(); return !!m && m.isStyleLoaded() && m.areTilesLoaded(); };
    const ll = typeof deviceMarker !== "undefined" && deviceMarker ? deviceMarker.getLatLng() : L.latLng(51.5079, -0.128);
    D.map.setView(ll, 16.4, { animate: false });
  };
  D.track = () => {
    if (!D.map || typeof deviceMarker === "undefined" || !deviceMarker) return;
    const ll = deviceMarker.getLatLng(), c = D.map.getCenter();
    if (Math.abs(ll.lat - c.lat) + Math.abs(ll.lng - c.lng) > 2e-6) D.map.setView(ll, 16.4, { animate: false });
  };
  const ready0 = window.__showreel && window.__showreel.mapReady;
  if (ready0) window.__showreel.mapReady = () => ready0() && D.glReady();

  D.openApp = (id) => {
    const p = D.at(id);
    D.el.app.style.setProperty("--ox", `${p.x}px`);
    D.el.app.style.setProperty("--oy", `${p.y}px`);
    D.el.app.classList.add("open");
    D.el.home.classList.add("behind");
    D.el.homebar.classList.add("dark");
  };
  D.closeApp = () => {
    D.el.app.classList.remove("open");
    D.el.home.classList.remove("behind");
    D.el.homebar.classList.remove("dark");
  };

  // ---- the agent's pointer
  D.cursor = (x, y) => { D.el.cursor.style.transform = `translate(${x}px, ${y}px)`; };
  D.cursorShow = (on) => D.el.cursor.classList.toggle("show", on);
  D.press = (down) => {
    D.el.cursor.classList.toggle("down", down);
    if (down) {
      const r = document.createElement("div");
      r.className = "dp-ripple";
      D.el.cursor.append(r);
      r.addEventListener("animationend", () => r.remove());
    }
  };
  D.holdRing = (on) => {
    D.el.cursor.querySelectorAll(".dp-hold").forEach((e) => e.remove());
    if (on) D.el.cursor.insertAdjacentHTML("afterbegin", '<svg class="dp-hold" viewBox="0 0 48 48"><circle cx="24" cy="24" r="22"/></svg>');
  };

  // ---- editing the home screen
  D.jiggle = (on) => D.el.home.classList.toggle("jiggle", on);
  D.lift = (id) => { const it = D.items.find((x) => x.id === id); it.el.classList.add("dragging"); };
  D.dragTo = (id, x, y) => { const it = D.items.find((x) => x.id === id); it.el.style.transform = `translate(${x}px, ${y}px)`; };
  // drop `id` on `target`: a new folder called `name` there, or into the folder that is there. The dragged
  // icon leaves the grid at once, so the others close the gap while it shrinks into the folder.
  D.drop = (id, target, name) => {
    const i = D.items.findIndex((x) => x.id === id);
    const it = D.items[i], tg = D.items.find((x) => x.id === target);
    D.items.splice(i, 1);
    tg.apps.push(...it.apps);
    const p = slot(D.items.indexOf(tg));
    it.el.classList.remove("dragging");
    it.el.style.transform = `translate(${p.x}px, ${p.y}px) scale(.35)`;
    it.el.classList.add("gone");
    setTimeout(() => it.el.remove(), 420);
    const lift = tg.el.querySelector(".dp-lift");
    lift.querySelector(".dp-tile").outerHTML = folderHTML(tg.apps);
    if (name) { tg.name = name; tg.el.querySelector(".dp-label").textContent = name; }
    const tile = lift.querySelector(".dp-tile");
    tile.classList.add("pop");
    setTimeout(() => tile.classList.remove("pop"), 170);
    D.layout(false);
  };

  // ------------------------------------------------------------------ the terminal
  const T = (D.term = {});
  T.install = (tools) => {
    const el = document.createElement("div");
    el.id = "demo-term";
    el.innerHTML = `<div class="dt-bar">${SPARK}<span>Claude Code</span><span class="path">~\\projects</span><span class="win"><span>—</span><span>▢</span><span>✕</span></span></div>
      <div class="dt-body">
        <div class="dt-welcome">${SPARK}<b>Welcome to Claude Code</b><br><span class="dt-dim">MCP server </span>idevbench<span class="dt-dim"> · </span><span class="dt-ok">connected</span><span class="dt-dim"> · ${tools} tools</span></div>
        <div class="dt-log"></div>
        <div class="dt-think">${SPARK}<span class="t"></span><span class="dt-dim">(esc to interrupt)</span></div>
        <div class="dt-input"><span class="txt"></span><span class="dt-caret"></span></div>
        <div class="dt-foot"><span class="dt-dim">? for shortcuts</span><span><span class="dt-ok">●</span> <span class="dt-dim">idevbench</span></span></div>
      </div>`;
    document.body.append(el);
    T.el = el;
    T.log = el.querySelector(".dt-log");
    T.txt = el.querySelector(".dt-input .txt");
  };
  T.open = () => T.el.classList.add("open");
  T.key = (ch) => { T.el.querySelector(".dt-input").classList.add("typing"); T.txt.textContent += ch; };
  T.submit = () => {
    const text = T.txt.textContent;
    T.txt.textContent = "";
    T.el.querySelector(".dt-input").classList.remove("typing");
    T.add("user", text);
  };
  T.add = (kind, html) => {
    const d = document.createElement("div");
    d.className = `dt-line dt-${kind}`;
    if (kind === "user") d.textContent = html;
    else d.innerHTML = (kind === "res" ? "" : '<span class="b"></span>') + html;
    T.log.append(d);
    // the oldest lines scroll away, as in a terminal
    while (T.log.scrollHeight > T.log.clientHeight + 1 && T.log.children.length > 1) T.log.firstElementChild.remove();
    return d;
  };
  const esc = (s) => String(s).replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
  T.say = (text) => T.add("say", esc(text));
  T.tool = (name, args) => T.add("tool", `<strong>idevbench - ${esc(name)} (MCP)</strong>${args ? `<span class="args">(${esc(args)})</span>` : ""}`);
  T.result = (text) => T.add("res", esc(text));
  T.think = (label) => {
    const t = T.el.querySelector(".dt-think");
    t.classList.toggle("on", !!label);
    if (label) t.querySelector(".t").textContent = `${label}…`;
  };
})();
