/* WorkLog 前端（vanilla JS + Tauri invoke） */
const TAURI = window.__TAURI__ || {};
const invoke = (TAURI.core && TAURI.core.invoke) || TAURI.invoke || (async () => { throw new Error('Tauri API 未注入'); });
const listen = (TAURI.event && TAURI.event.listen) || (async () => () => {});

const CATEGORY_COLORS = {
  conversion: '#6366f1', insight: '#0ea5e9', script: '#f59e0b', review: '#10b981',
  data_ops: '#8b5cf6', parent_comm: '#ec4899', dev: '#64748b', writing: '#14b8a6',
  meeting: '#3b82f6', research: '#a855f7', browsing: '#94a3b8',
};
const CATEGORY_LABELS = {
  conversion: '转化运营', insight: '学情采集', script: '话术库', review: '复盘分析',
  data_ops: '数据运营', parent_comm: '家长沟通', dev: '开发', writing: '文档',
  meeting: '会议', research: '学习', browsing: '生活', idle: '其他', other: '其他',
};

let settings = {};
let currentRecords = [];
let currentReport = '';
let currentDayTab = 'today';

function esc(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}
function fmtHM(ts) { return (ts || '').slice(11, 16) || '--:--'; }
function p2(n) { return String(n).padStart(2, '0'); }
function todayStr() { const d = new Date(); return `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())}`; }
function dayStr(offset) {
  const d = new Date(); d.setDate(d.getDate() + offset);
  return `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())}`;
}
function chip(activity) {
  const color = CATEGORY_COLORS[activity] || '#94a3b8';
  const label = CATEGORY_LABELS[activity] || '其他';
  return `<span class="chip" style="color:${color};background:${color}18">${label}</span>`;
}

/* ============ 视图切换 ============ */
function switchView(name) {
  document.querySelectorAll('.nav-item').forEach(b => b.classList.toggle('active', b.dataset.view === name));
  document.querySelectorAll('.view').forEach(v => v.classList.add('hidden'));
  document.getElementById('view-' + name).classList.remove('hidden');
  if (name === 'overview') refreshOverview();
  if (name === 'today') refreshDayList();
  if (name === 'report') refreshReportDates().then(d => loadReport(d, false));
  if (name === 'weekly') refreshWeeks().then(w => loadWeekly(w, false));
  if (name === 'stats') refreshStats();
}

/* ============ 概览 ============ */
async function refreshOverview() {
  try {
    const st = await invoke('stats_overview');
    currentRecords = await invoke('get_today_records');
    renderOverviewStats(st);
    renderTimeline();
    renderDirections(st.distribution || []);
    renderCalendar();
  } catch (e) { /* ignore */ }
}

function renderOverviewStats(st) {
  document.getElementById('ovCount').innerHTML = `${st.today_count}<span class="stat-unit">条</span>`;
  const diff = st.today_count - st.yesterday_count;
  const diffEl = document.getElementById('ovCountDiff');
  diffEl.textContent = diff === 0 ? '与昨日持平' : (diff > 0 ? `较昨日 +${diff}` : `较昨日 ${diff}`);
  diffEl.className = 'stat-foot ' + (diff > 0 ? 'up' : (diff < 0 ? 'down' : ''));
  document.getElementById('ovThemes').innerHTML = `${st.themes}<span class="stat-unit">个</span>`;
  document.getElementById('ovFocus').innerHTML = `${st.focus_hours_today}<span class="stat-unit">小时</span>`;
  const fd = (st.focus_hours_today - st.focus_hours_yesterday);
  const fdEl = document.getElementById('ovFocusDiff');
  fdEl.textContent = fd === 0 ? '与昨日持平' : (fd > 0 ? `较昨日 +${fd.toFixed(1)}h` : `较昨日 ${fd.toFixed(1)}h`);
  fdEl.className = 'stat-foot ' + (fd > 0 ? 'up' : (fd < 0 ? 'down' : ''));
  // 日报状态：已推送 > 待推送 > 待记录（不能只看记录数，推送完还得显示已推送）
  const pushEl = document.getElementById('ovPush');
  if (st.today_pushed) {
    pushEl.textContent = '已推送';
    pushEl.className = 'stat-value push-ok';
  } else {
    pushEl.textContent = st.today_count > 0 ? '待推送' : '待记录';
    pushEl.className = 'stat-value';
  }
  document.getElementById('ovPushTime').textContent = `${p2(settings.feishu_push_hour ?? 18)}:${p2(settings.feishu_push_minute ?? 10)} 自动推送`;
  const d = new Date();
  const wk = ['日', '一', '二', '三', '四', '五', '六'][d.getDay()];
  document.getElementById('greetingSub').textContent =
    `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日 · 星期${wk}　专注当下，记录成长的每一步 ✨`;
}

function renderTimeline() {
  const box = document.getElementById('timeline');
  if (!currentRecords.length) {
    box.innerHTML = `<div class="empty"><div class="empty-emoji">🛰️</div><p>开始记录后，WorkLog 会每隔几分钟截屏分析<br/>自动沉淀工作轨迹（只保留值得汇报的内容）</p></div>`;
    return;
  }
  box.innerHTML = currentRecords.map((r) => {
    const meta = [];
    if (r.progress) meta.push(`<span class="tl-progress">✅ ${esc(r.progress)}</span>`);
    if (r.todo) meta.push(`<span class="tl-todo">📌 ${esc(r.todo)}</span>`);
    return `<div class="tl-item">
      <div class="tl-time">${fmtHM(r.ts)}</div>
      <div class="tl-main">
        <div class="tl-summary">${esc(r.summary)}</div>
        ${r.detail ? `<div class="tl-detail">${esc(r.detail)}</div>` : ''}
        <div class="tl-meta">${chip(r.activity)}${meta.join('')}</div>
      </div>
    </div>`;
  }).join('');
}

function renderDirections(dist) {
  const box = document.getElementById('dirList');
  if (!dist.length) {
    box.innerHTML = `<div class="empty" style="padding:18px 0"><p>暂无业务方向记录</p></div>`;
    return;
  }
  const max = Math.max(...dist.map(d => d.count), 1);
  box.innerHTML = dist.slice(0, 6).map((d) => `
    <div class="dir-row">
      <div class="dir-top"><span class="dir-name">${esc(d.label)}</span><span class="dir-count">${d.count} 次</span></div>
      <div class="bar"><i style="width:${Math.round(d.count / max * 100)}%"></i></div>
    </div>`).join('');
}

function renderCalendar() {
  const d = new Date();
  const y = d.getFullYear(), m = d.getMonth();
  document.getElementById('calTitle').textContent = `${y}年${m + 1}月`;
  const first = new Date(y, m, 1).getDay();
  const daysInMonth = new Date(y, m + 1, 0).getDate();
  const prevDays = new Date(y, m, 0).getDate();
  const recDates = new Set(currentRecords.map(r => r.ts.slice(0, 10)));
  let cells = '<div class="cal-head"><span>日</span><span>一</span><span>二</span><span>三</span><span>四</span><span>五</span><span>六</span></div><div class="cal-grid">';
  for (let i = 0; i < first; i++) cells += `<div class="cal-day other">${prevDays - first + i + 1}</div>`;
  for (let day = 1; day <= daysInMonth; day++) {
    const ds = `${y}-${p2(m + 1)}-${p2(day)}`;
    const isToday = ds === todayStr();
    const has = recDates.has(ds);
    cells += `<div class="cal-day ${isToday ? 'today' : ''} ${has ? 'has-rec' : ''}">${day}</div>`;
  }
  const total = first + daysInMonth;
  for (let i = 1; i <= (7 - total % 7) % 7; i++) cells += `<div class="cal-day other">${i}</div>`;
  cells += '</div>';
  document.getElementById('calendar').innerHTML = cells;
}

/* ============ 今日记录（日报列表） ============ */
async function refreshDayList() {
  try {
    const list = await invoke('day_list');
    currentDayTab = currentDayTab || 'today';
    renderDayList(list);
  } catch (e) { /* ignore */ }
}

function renderDayList(list) {
  const today = todayStr();
  const map = { today, yesterday: dayStr(-1), before: dayStr(-2) };
  let filtered = list;
  if (currentDayTab !== 'all') {
    filtered = list.filter(d => d.date === map[currentDayTab]);
  }
  const box = document.getElementById('dayList');
  if (!filtered.length) {
    box.innerHTML = `<div class="empty"><div class="empty-emoji">📭</div><p>这天还没有工作记录</p></div>`;
    return;
  }
  box.innerHTML = filtered.map(d => {
    const dt = new Date(d.date + 'T00:00:00');
    const wk = ['日', '一', '二', '三', '四', '五', '六'][dt.getDay()];
    const push = d.pushed ? '<span class="day-pill ok">已推送</span>'
      : (d.has_report ? '<span class="day-pill warn">已生成</span>' : '<span class="day-pill">未推送</span>');
    return `<div class="day-row">
      <div class="day-date">${d.date} · 星期${wk}</div>
      <div class="day-meta">记录 ${d.count} 条 · ${d.hours} 小时</div>
      ${push}
      <button class="day-act" data-date="${d.date}">查看 →</button>
    </div>`;
  }).join('');
  box.querySelectorAll('.day-act').forEach(b => b.addEventListener('click', () => {
    switchView('report');
    document.getElementById('reportDateSelect').value = b.dataset.date;
    loadReport(b.dataset.date, false);
  }));
}

/* ============ 日报 ============ */
async function refreshReportDates() {
  try {
    const dates = await invoke('report_dates');
    const sel = document.getElementById('reportDateSelect');
    const keep = sel.value;
    sel.innerHTML = (dates.length ? dates : [todayStr()])
      .map(d => `<option value="${d}">${d}${d === todayStr() ? '（今天）' : ''}</option>`).join('');
    if (keep && (dates.includes(keep) || keep === todayStr())) sel.value = keep;
    else if (sel.options.length) sel.value = sel.options[0].value;
    return sel.value;
  } catch (e) { return todayStr(); }
}

async function loadReport(date, regenerate) {
  const box = document.getElementById('reportFull');
  const btn = document.getElementById('genReportBtn');
  const select = document.getElementById('reportDateSelect');
  if (btn && regenerate) { btn.disabled = true; btn.dataset.oldText = btn.innerHTML; btn.innerHTML = '生成中…'; }
  if (select && regenerate) select.disabled = true;
  box.innerHTML = '<div class="empty"><div class="empty-emoji">⏳</div><p>正在生成日报…（首次约 20 秒）</p></div>';
  try {
    const text = await invoke('get_report', { date, regenerate: !!regenerate });
    currentReport = text;
    box.innerHTML = `<textarea id="reportEditor" class="report-editor" spellcheck="false">${esc(text)}</textarea>`;
  } catch (e) {
    box.innerHTML = `<div class="empty"><div class="empty-emoji">⚠️</div><p>${esc(String(e))}</p></div>`;
  } finally {
    if (btn && regenerate) { btn.disabled = false; btn.innerHTML = btn.dataset.oldText || '✨ 生成日报'; }
    if (select && regenerate) select.disabled = false;
  }
}

/* ============ 周报 ============ */
async function refreshWeeks() {
  try {
    const weeks = await invoke('week_list');
    const sel = document.getElementById('weekSelect');
    const keep = sel.value;
    sel.innerHTML = weeks.map(w => `<option value="${w.start}">${w.label}</option>`).join('');
    if (keep && weeks.some(w => w.start === keep)) sel.value = keep;
    else if (sel.options.length) sel.value = sel.options[0].value;
    return sel.value;
  } catch (e) { return null; }
}

async function loadWeekly(weekStart, regenerate) {
  if (!weekStart) return;
  const box = document.getElementById('weeklyBody');
  const btn = document.getElementById('genWeeklyBtn');
  const select = document.getElementById('weekSelect');
  if (btn && regenerate) { btn.disabled = true; btn.dataset.oldText = btn.innerHTML; btn.innerHTML = '生成中…'; }
  if (select && regenerate) select.disabled = true;
  box.innerHTML = '<div class="empty"><div class="empty-emoji">⏳</div><p>正在生成周报…</p></div>';
  try {
    const text = await invoke('get_weekly_report', { weekStart, regenerate: !!regenerate });
    box.innerHTML = `<div class="report-full">${esc(text)}</div>`;
  } catch (e) {
    box.innerHTML = `<div class="empty"><div class="empty-emoji">⚠️</div><p>${esc(String(e))}</p></div>`;
  } finally {
    if (btn && regenerate) { btn.disabled = false; btn.innerHTML = btn.dataset.oldText || '✨ 生成周报'; }
    if (select && regenerate) select.disabled = false;
  }
}

/* ============ 数据分析 ============ */
async function refreshStats() {
  try {
    const st = await invoke('stats_overview');
    document.getElementById('stTotal').innerHTML = `${st.total_records}<span class="stat-unit">条</span>`;
    document.getElementById('stTotalFoot').textContent = `共 ${st.work_days} 天有记录`;
    document.getElementById('stDays').innerHTML = `${st.work_days}<span class="stat-unit">天</span>`;
    document.getElementById('stAvg').innerHTML = `${st.avg_hours}<span class="stat-unit">小时/天</span>`;
    document.getElementById('stRate').innerHTML = `${st.record_rate}<span class="stat-unit">%</span>`;
    drawTrend(st.daily_counts || []);
    drawDonut(st.distribution || []);
  } catch (e) { /* ignore */ }
}

function drawTrend(data) {
  const canvas = document.getElementById('trendChart');
  const dpr = window.devicePixelRatio || 1;
  const W = canvas.clientWidth || 480, H = 220;
  canvas.width = W * dpr; canvas.height = H * dpr;
  const ctx = canvas.getContext('2d');
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, W, H);
  const pad = { l: 34, r: 12, t: 14, b: 26 };
  const max = Math.max(...data.map(d => d.count), 5);
  const iw = W - pad.l - pad.r, ih = H - pad.t - pad.b;
  // 网格
  ctx.strokeStyle = '#eef0f7'; ctx.lineWidth = 1;
  for (let i = 0; i <= 4; i++) {
    const y = pad.t + ih * i / 4;
    ctx.beginPath(); ctx.moveTo(pad.l, y); ctx.lineTo(W - pad.r, y); ctx.stroke();
    ctx.fillStyle = '#a0a4b8'; ctx.font = '10px sans-serif';
    ctx.fillText(String(Math.round(max * (4 - i) / 4)), 6, y + 3);
  }
  // 折线
  const pts = data.map((d, i) => ({
    x: pad.l + iw * (data.length > 1 ? i / (data.length - 1) : 0.5),
    y: pad.t + ih - ih * d.count / max,
  }));
  const grad = ctx.createLinearGradient(0, pad.t, 0, H);
  grad.addColorStop(0, 'rgba(99,102,241,.28)');
  grad.addColorStop(1, 'rgba(99,102,241,0)');
  if (pts.length) {
    ctx.beginPath();
    ctx.moveTo(pts[0].x, H - pad.b);
    pts.forEach(p => ctx.lineTo(p.x, p.y));
    ctx.lineTo(pts[pts.length - 1].x, H - pad.b);
    ctx.closePath(); ctx.fillStyle = grad; ctx.fill();
    ctx.beginPath();
    pts.forEach((p, i) => i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y));
    ctx.strokeStyle = '#6366f1'; ctx.lineWidth = 2.2; ctx.lineJoin = 'round'; ctx.stroke();
    pts.forEach(p => {
      ctx.beginPath(); ctx.arc(p.x, p.y, 3.2, 0, Math.PI * 2);
      ctx.fillStyle = '#fff'; ctx.fill();
      ctx.strokeStyle = '#6366f1'; ctx.lineWidth = 2; ctx.stroke();
    });
  }
  // X 轴
  ctx.fillStyle = '#a0a4b8'; ctx.font = '10px sans-serif'; ctx.textAlign = 'center';
  data.forEach((d, i) => {
    const x = pad.l + iw * (data.length > 1 ? i / (data.length - 1) : 0.5);
    ctx.fillText(d.date.slice(5), x, H - 8);
  });
  ctx.textAlign = 'left';
}

function drawDonut(dist) {
  const donut = document.getElementById('donut');
  const legend = document.getElementById('donutLegend');
  if (!dist.length) {
    donut.style.background = '#eef0f7';
    legend.innerHTML = '<div class="legend-row">暂无数据</div>';
    return;
  }
  const total = dist.reduce((s, d) => s + d.count, 0);
  let acc = 0;
  const stops = [];
  dist.forEach((d) => {
    const c = CATEGORY_COLORS[Object.keys(CATEGORY_LABELS).find(k => CATEGORY_LABELS[k] === d.label)] || '#94a3b8';
    const start = acc / total * 360, end = (acc + d.count) / total * 360;
    stops.push(`${c} ${start}deg ${end}deg`);
    acc += d.count;
  });
  donut.style.background = `conic-gradient(${stops.join(',')})`;
  legend.innerHTML = dist.slice(0, 6).map(d => {
    const k = Object.keys(CATEGORY_LABELS).find(k => CATEGORY_LABELS[k] === d.label);
    const c = CATEGORY_COLORS[k] || '#94a3b8';
    return `<div class="legend-row"><span class="legend-dot" style="background:${c}"></span>${esc(d.label)} ${Math.round(d.count / total * 100)}%</div>`;
  }).join('');
}

/* ============ 设置 ============ */
async function loadSettings() {
  settings = await invoke('get_settings');
  const s = settings;
  document.getElementById('setInterval').value = String(s.interval_minutes);
  document.getElementById('setExclude').value = s.exclude_keywords || '';
  document.getElementById('setBase').value = s.model_base_url || '';
  document.getElementById('setModel').value = s.model_name || '';
  document.getElementById('setKey').value = s.model_api_key || '';
  document.getElementById('setDept').value = s.feishu_dept || '';
  document.getElementById('setName').value = s.feishu_user_name || '';
  document.getElementById('setOpenId').value = s.feishu_user_open_id || '';
  document.getElementById('setAutoPush').value = s.feishu_auto_push_enabled ? '1' : '0';
  document.getElementById('setPushHour').value = String(s.feishu_push_hour ?? 18);
  document.getElementById('setPushMinute').value = String(s.feishu_push_minute ?? 10);
  document.getElementById('setDocToken').value = s.knowledge_doc_token || '';
  document.getElementById('setArchive').value = s.knowledge_archive_enabled ? '1' : '0';
  document.getElementById('setAutoVc').value = s.meeting_auto_vc ? '1' : '0';
  document.getElementById('setMeetingLinks').value = s.meeting_links || '';
  document.getElementById('setMeetingNotes').value = s.meeting_notes || '';
  document.getElementById('setTemplate').value = s.report_template || '';
  document.getElementById('setWeeklyTemplate').value = s.weekly_template || '';
  document.getElementById('setReference').value = s.reference_samples || '';
  document.getElementById('setDirections').value = (s.work_directions || []).join('\n');
  await refreshRefStats();
}

async function refreshRefStats() {
  try {
    const st = await invoke('reference_stats');
    document.getElementById('refStats').textContent =
      `知识沉淀：${st.samples} 份样本 · ${st.reference_chars} 字 · 模板 ${st.template_chars} 字`;
  } catch (e) { /* ignore */ }
}

async function saveSettings() {
  const hint = document.getElementById('saveHint');
  try {
    const s = Object.assign({}, settings);
    s.interval_minutes = Number(document.getElementById('setInterval').value);
    s.exclude_keywords = document.getElementById('setExclude').value;
    s.model_base_url = document.getElementById('setBase').value.trim();
    s.model_name = document.getElementById('setModel').value.trim();
    s.model_api_key = document.getElementById('setKey').value.trim();
    s.feishu_dept = document.getElementById('setDept').value.trim();
    s.feishu_user_name = document.getElementById('setName').value.trim();
    s.feishu_user_open_id = document.getElementById('setOpenId').value.trim();
    s.feishu_auto_push_enabled = document.getElementById('setAutoPush').value === '1';
    s.feishu_push_hour = Number(document.getElementById('setPushHour').value);
    s.feishu_push_minute = Number(document.getElementById('setPushMinute').value);
    s.knowledge_doc_token = document.getElementById('setDocToken').value.trim();
    s.knowledge_archive_enabled = document.getElementById('setArchive').value === '1';
    s.meeting_auto_vc = document.getElementById('setAutoVc').value === '1';
    s.meeting_links = document.getElementById('setMeetingLinks').value;
    s.meeting_notes = document.getElementById('setMeetingNotes').value;
    s.report_template = document.getElementById('setTemplate').value;
    s.weekly_template = document.getElementById('setWeeklyTemplate').value;
    s.reference_samples = document.getElementById('setReference').value;
    s.work_directions = document.getElementById('setDirections').value.split('\n').map(x => x.trim()).filter(Boolean);
    await invoke('save_settings', { settings: s });
    settings = await invoke('get_settings');
    await loadSettings();
    hint.textContent = `✅ 已保存并校验（间隔 ${settings.interval_minutes} 分钟 · 推送 ${p2(settings.feishu_push_hour)}:${p2(settings.feishu_push_minute)}）`;
  } catch (e) {
    hint.textContent = '⚠️ 保存失败：' + e;
    hint.style.color = '#dc2626';
    setTimeout(() => (hint.style.color = ''), 6000);
  }
  setTimeout(() => (hint.textContent = ''), 6000);
}

/* ============ 推送 / 记录状态 ============ */
async function pushNow(force) {
  const btn = document.getElementById('pushBtn');
  const old = btn.innerHTML;
  btn.disabled = true;
  btn.innerHTML = '推送中…';
  try {
    const r = await invoke('push_now', { force: !!force });
    if (r.ok) {
      btn.innerHTML = '✅ 已推送到飞书';
      const pill = document.getElementById('pillPush');
      pill.textContent = '飞书：已推送';
      pill.className = 'pill ok';
      document.getElementById('ovPush').textContent = '已推送';
    } else {
      btn.innerHTML = old;
      alert('推送失败：' + r.reason);
    }
    setTimeout(() => (btn.innerHTML = old), 2500);
  } catch (e) {
    btn.innerHTML = old;
    btn.disabled = false;
    alert('推送失败：' + e);
  }
  btn.disabled = false;
}

async function renderRecState() {
  try {
    const on = await invoke('get_recording');
    document.getElementById('recDot').className = on ? 'dot' : 'dot off';
    document.getElementById('recText').textContent = on ? '记录中' : '已暂停';
    const pill = document.getElementById('pillRec');
    pill.className = on ? 'pill ok' : 'pill';
    pill.innerHTML = on ? '<span class="dot"></span>记录中' : '<span class="dot off"></span>已暂停';
  } catch (e) { /* ignore */ }
}

/* ============ 初始化 ============ */
function fillTimeSelects() {
  const h = document.getElementById('setPushHour');
  const m = document.getElementById('setPushMinute');
  for (let i = 0; i < 24; i++) h.insertAdjacentHTML('beforeend', `<option value="${i}">${p2(i)} 时</option>`);
  for (let i = 0; i < 60; i += 5) m.insertAdjacentHTML('beforeend', `<option value="${i}">${p2(i)} 分</option>`);
}

function init() {
  fillTimeSelects();
  document.querySelectorAll('.nav-item').forEach(b =>
    b.addEventListener('click', () => switchView(b.dataset.view)));

  document.getElementById('pushBtn').addEventListener('click', () => pushNow(false));
  document.getElementById('qaRefresh').addEventListener('click', () => { switchView('report'); });
  document.getElementById('qaReport').addEventListener('click', () => switchView('report'));
  document.getElementById('qaStats').addEventListener('click', () => switchView('stats'));
  document.getElementById('qaSettings').addEventListener('click', () => switchView('settings'));

  document.querySelectorAll('#dayTabs .tab').forEach(t => t.addEventListener('click', () => {
    document.querySelectorAll('#dayTabs .tab').forEach(x => x.classList.remove('active'));
    t.classList.add('active');
    currentDayTab = t.dataset.day;
    refreshDayList();
  }));

  document.getElementById('genReportBtn').addEventListener('click', () => {
    loadReport(document.getElementById('reportDateSelect').value, true);
  });
  document.getElementById('reportDateSelect').addEventListener('change', (e) => loadReport(e.target.value, false));
  document.getElementById('copyReport').addEventListener('click', () => {
    const editor = document.getElementById('reportEditor');
    navigator.clipboard.writeText(editor ? editor.value : currentReport);
  });
  document.getElementById('saveReportFeedback').addEventListener('click', async () => {
    const editor = document.getElementById('reportEditor');
    const date = document.getElementById('reportDateSelect').value;
    if (!editor || !date) return;
    try {
      const msg = await invoke('save_report_feedback', { date, body: editor.value });
      currentReport = editor.value;
      const hint = document.getElementById('reportFeedbackHint');
      if (hint) { hint.textContent = msg; setTimeout(() => { hint.textContent = ''; }, 4000); }
      await refreshRefStats();
    } catch (e) {
      const hint = document.getElementById('reportFeedbackHint');
      if (hint) { hint.textContent = String(e); }
    }
  });

  document.getElementById('genWeeklyBtn').addEventListener('click', () => {
    loadWeekly(document.getElementById('weekSelect').value, true);
  });
  document.getElementById('weekSelect').addEventListener('change', (e) => loadWeekly(e.target.value, false));
  document.getElementById('copyWeekly').addEventListener('click', () => {
    navigator.clipboard.writeText(document.getElementById('weeklyBody').innerText);
  });

  document.getElementById('saveBtn').addEventListener('click', saveSettings);
  document.getElementById('previewMeetingBtn').addEventListener('click', async () => {
    const h = document.getElementById('meetingHint');
    h.textContent = '读取中…';
    try {
      const t = await invoke('fetch_meetings');
      h.textContent = t.slice(0, 80) + (t.length > 80 ? `…（共 ${t.length} 字）` : '');
    } catch (e) { h.textContent = '⚠️ ' + e; }
    setTimeout(() => (h.textContent = ''), 6000);
  });

  loadSettings();
  renderRecState();
  refreshOverview();
  setInterval(() => { if (!document.getElementById('view-overview').classList.contains('hidden')) refreshOverview(); }, 30000);
  setInterval(renderRecState, 5000);
  listen('record-updated', () => {
    if (!document.getElementById('view-overview').classList.contains('hidden')) refreshOverview();
  });
  listen('record-state-changed', () => renderRecState());
  listen('push-result', (e) => {
    if (e.payload && e.payload.ok) {
      const pill = document.getElementById('pillPush');
      pill.textContent = '飞书：已推送';
      pill.className = 'pill ok';
    }
  });
}

window.addEventListener('DOMContentLoaded', init);
