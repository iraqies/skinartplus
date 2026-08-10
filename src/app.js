let state = {
  inputPath: null,
  baseSkinPath: null,
  originalSkinPath: null,
  originalSkinHead: null,
  lastTileDataUrl: null,
  skins: [],
  bearerToken: null,
  refreshToken: null,
  ign: null,
  uuid: null,
  pollTimer: null,
  uploadRunning: false,
  claiming: false,
  templates: [],
  tileData: {},
  skinModel: null,
  originalSkinSource: null,
  usedTemplate: null,
};

const dom = {
  inputName: document.getElementById('input-name'),
  baseName: document.getElementById('base-name'),
  originalName: document.getElementById('original-name'),
  inputZone: document.getElementById('input-zone'),
  baseZone: document.getElementById('base-zone'),
  originalZone: document.getElementById('original-zone'),
  previewWrapper: document.getElementById('preview-wrapper'),
  btnGenerate: document.getElementById('btn-generate'),
  stepUpload: document.getElementById('step-upload'),
  stepDone: document.getElementById('step-done'),
  btnStart: document.getElementById('btn-start-upload'),
  btnExport: document.getElementById('btn-export'),
  uploadGrid: document.getElementById('upload-grid'),
  confirmArea: document.getElementById('confirm-area'),
  confirmTitle: document.getElementById('confirm-title'),
  confirmHint: document.getElementById('confirm-hint'),
  confirmHead: document.getElementById('confirm-head'),
  btnSkipWait: document.getElementById('btn-skip-wait'),
  pollStatus: document.getElementById('poll-status'),
  pollText: document.getElementById('poll-text'),
  btnRestart: document.getElementById('btn-restart'),
  templatesGrid: document.getElementById('templates-grid'),
  accountArea: document.getElementById('account-area'),
  btnSignin: document.getElementById('btn-signin'),
  signinModal: document.getElementById('signin-modal'),
  btnCloseModal: document.getElementById('btn-close-modal'),
  btnAuthStart: document.getElementById('btn-auth-start'),
  deviceCodeArea: document.getElementById('device-code-area'),
  dcUri: document.getElementById('dc-uri'),
  dcCode: document.getElementById('dc-code'),
  btnOpenBrowser: document.getElementById('btn-open-browser'),
  dcStatus: document.getElementById('dc-status'),
  dcSpinner: document.getElementById('dc-spinner'),
  savedAccountsSection: document.getElementById('saved-accounts-section'),
  savedAccountsList: document.getElementById('saved-accounts-list'),
  accountDropdown: document.getElementById('account-dropdown'),
  ddHead: document.getElementById('dd-head'),
  ddName: document.getElementById('dd-name'),
  ddUuid: document.getElementById('dd-uuid'),
  ddNamemc: document.getElementById('dd-namemc'),
  ddSwitch: document.getElementById('dd-switch'),
  ddLogout: document.getElementById('dd-logout'),
};

// ── Tab Switching ────────────────────────────────────────────────

document.querySelectorAll('.tab-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    btn.classList.add('active');
    document.getElementById('tab-' + btn.dataset.tab).classList.add('active');
  });
});

// ── Template Search ──────────────────────────────────────────────

const templateSearchInput = document.getElementById('templates-search');
const templateTagSelect = document.getElementById('templates-tag-select');

function buildTagDropdown() {
  const tagSet = new Set();
  for (const t of state.templates) {
    for (const tag of (t.tags || [])) {
      tagSet.add(tag);
    }
  }
  const sorted = [...tagSet].sort();
  templateTagSelect.innerHTML = '<option value="">All Tags</option>';
  for (const tag of sorted) {
    const opt = document.createElement('option');
    opt.value = tag;
    opt.textContent = tag.charAt(0).toUpperCase() + tag.slice(1);
    templateTagSelect.appendChild(opt);
  }
}

templateSearchInput.addEventListener('input', () => renderTemplates());
templateTagSelect.addEventListener('change', () => renderTemplates());

// ── File Selection ───────────────────────────────────────────────

dom.inputZone.querySelector('#select-input').addEventListener('click', async () => {
  const p = await window.__TAURI__.core.invoke('select_image');
  if (p) {
    state.inputPath = p;
    state.usedTemplate = null;
    dom.inputName.textContent = p.split('\\').pop();
    dom.inputZone.classList.add('has-file');
    updateGenerateBtn();
    renderPreview();
  }
});

dom.baseZone.querySelector('#select-base').addEventListener('click', async () => {
  const p = await window.__TAURI__.core.invoke('select_base_skin');
  if (p) {
    state.baseSkinPath = p;
    dom.baseName.textContent = p.split('\\').pop();
    dom.baseZone.classList.add('has-file');
  } else {
    state.baseSkinPath = null;
    dom.baseName.textContent = 'None';
    dom.baseZone.classList.remove('has-file');
  }
});

dom.originalZone.querySelector('#select-original').addEventListener('click', async () => {
  const p = await window.__TAURI__.core.invoke('select_original_skin');
  if (p) {
    setOriginalSkin(p, p.split('\\').pop());
    state.originalSkinSource = 'manual';
    state.skinModel = await askSkinModel();
  } else {
    clearOriginalSkin();
  }
});

async function readFileAsDataUrl(filePath) {
  try {
    const res = await window.__TAURI__.core.invoke('read_file_base64', { filePath });
    if (res && res.success && res.data) return 'data:image/png;base64,' + res.data;
  } catch {}
  return null;
}

async function setOriginalSkin(filePath, displayName) {
  state.originalSkinPath = filePath;
  state.originalSkinHead = null;
  dom.originalName.textContent = displayName + ' -> #27';
  dom.originalZone.classList.add('has-file');
  const dataUrl = await readFileAsDataUrl(filePath);
  if (!dataUrl) return;
  const img = new Image();
  img.src = dataUrl;
  img.onload = () => {
    const c = document.createElement('canvas');
    c.width = 64; c.height = 64;
    const ctx = c.getContext('2d');
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(img, 8, 8, 8, 8, 0, 0, 64, 64);
    ctx.drawImage(img, 40, 8, 8, 8, 0, 0, 64, 64);
    state.originalSkinHead = c.toDataURL();
    updateCell27();
    updatePreviewCell27();
  };
}

function clearOriginalSkin() {
  state.originalSkinPath = null;
  state.originalSkinHead = null;
  dom.originalName.textContent = 'Defaults to last tile from art';
  dom.originalZone.classList.remove('has-file');
  updateCell27();
  updatePreviewCell27();
}

function updatePreviewCell27() {
  const cells = dom.previewWrapper.querySelectorAll('.preview-cell');
  if (cells.length < 27) return;
  const cell27 = cells[0];
  if (!cell27) return;
  const img = cell27.querySelector('img');
  const label = cell27.querySelector('.pl');
  if (state.originalSkinHead && img) {
    img.src = state.originalSkinHead;
    if (label) { label.textContent = 'FACE'; label.className = 'pl'; }
  } else if (state.lastTileDataUrl && img) {
    img.src = state.lastTileDataUrl;
    if (label) { label.textContent = '27'; label.className = 'pl'; }
  } else if (state.tileData && state.tileData[27] && img) {
    img.src = state.tileData[27];
    if (label) { label.textContent = '27'; label.className = 'pl'; }
  } else if (img) {
    img.src = '';
    if (label) { label.textContent = ''; label.className = 'pl'; }
  }
}

function updateGenerateBtn() {
  dom.btnGenerate.disabled = !state.inputPath;
}

// ── Generate ─────────────────────────────────────────────────────

dom.btnGenerate.addEventListener('click', async () => {
  dom.btnGenerate.disabled = true;
  dom.btnGenerate.textContent = 'Generating...';
  try {
    const result = await window.__TAURI__.core.invoke('generate_all', {
      opts: {
        inputPath: state.inputPath,
        baseSkinPath: state.baseSkinPath,
        originalSkinPath: state.originalSkinPath,
        showNumbers: settings.skinNumbers
      }
    });
    state.skins = result.skins;
    state.outputDir = result.outputDir;
    switchToUpload();
  } catch (e) {
    alert('Error: ' + e.message);
  }
  dom.btnGenerate.textContent = 'Generate Skins';
  dom.btnGenerate.disabled = false;
});

function switchToUpload() {
  document.getElementById('tab-create').classList.remove('active');
  document.getElementById('tab-templates').classList.remove('active');
  document.getElementById('tab-design').classList.remove('active');
  document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
  document.querySelector('.tab-buttons').style.display = 'none';
  dom.stepUpload.classList.add('active');
  setupGrid();
  dom.btnExport.disabled = false;
  dom.btnStart.disabled = false;
  dom.btnStart.textContent = 'Start Upload';
}

// ── Templates ────────────────────────────────────────────────────

async function loadTemplates() {
  state.templates = await window.__TAURI__.core.invoke('load_templates');
  buildTagDropdown();
  renderTemplates();
}

function renderTemplates() {
  if (!state.templates.length) {
    dom.templatesGrid.innerHTML = '<p class="templates-empty">No templates available.</p>';
    return;
  }

  const query = templateSearchInput ? templateSearchInput.value.trim().toLowerCase() : '';
  const tagFilter = templateTagSelect ? templateTagSelect.value : '';

  let filtered = state.templates;

  if (tagFilter) {
    filtered = filtered.filter(t => (t.tags || []).includes(tagFilter));
  }

  if (query) {
    filtered = filtered.filter(t => {
      const name = (t.name || '').toLowerCase();
      const uploader = (t.uploader || '').toLowerCase();
      const tags = (t.tags || []).join(' ').toLowerCase();
      return name.includes(query) || uploader.includes(query) || tags.includes(query);
    });
  }

  if (!filtered.length) {
    dom.templatesGrid.innerHTML = '<p class="templates-empty">' + (query || selectedTag ? 'No matching templates.' : 'No templates available.') + '</p>';
    return;
  }
  dom.templatesGrid.innerHTML = '';
  for (const t of filtered) {
    const card = document.createElement('div');
    card.className = 'template-card';
    const headUrl = t.uuid ? 'https://mc-heads.net/avatar/' + t.uuid + '/64' : '';
    card.innerHTML =
      '<div class="template-preview" id="tpl-preview-' + t.id + '">' +
      '</div>' +
      '<div class="template-card-header">' +
        (headUrl ? '<img class="template-card-head" src="' + headUrl + '" alt="">' : '') +
        '<div class="template-card-info">' +
          '<div class="template-card-name">' + escHtml(t.name) + '</div>' +
          (t.uploader ? '<div class="template-card-uploader">by ' + escHtml(t.uploader) + '</div>' : '') +
        '</div>' +
      '</div>';

    card.addEventListener('click', async () => {
      const imgPath = await window.__TAURI__.core.invoke('get_template_image_path', { id: t.id });
      if (imgPath) {
        state.inputPath = imgPath;
        state.usedTemplate = { id: t.id, name: t.name };
        dom.inputName.textContent = t.name;
        dom.inputZone.classList.add('has-file');
        updateGenerateBtn();
        renderPreview();
        document.querySelector('[data-tab="create"]').click();
      }
    });

    dom.templatesGrid.appendChild(card);

    window.__TAURI__.core.invoke('get_template_image_data', { id: t.id }).then(data => {
      if (data) {
        const previewEl = document.getElementById('tpl-preview-' + t.id);
        if (previewEl) {
          previewEl.innerHTML = '<img src="data:image/png;base64,' + data + '" alt="' + escHtml(t.name) + '">';
        }
      }
    });
  }
}

function escHtml(s) {
  const d = document.createElement('div');
  d.textContent = s;
  return d.innerHTML;
}

// ── Account Area / Auth ──────────────────────────────────────────

async function updateAccountArea() {
  if (state.ign && state.uuid) {
    const avatar = await window.__TAURI__.core.invoke('fetch_avatar', { id: state.uuid });
    const src = avatar.success ? avatar.dataUrl : '';
    dom.accountArea.innerHTML =
      '<div class="account-pill" id="account-pill">' +
        '<img class="account-head" src="' + src + '" alt="">' +
        '<span class="account-name">' + escHtml(state.ign) + '</span>' +
        '<span class="account-arrow">&#9662;</span>' +
      '</div>';
    document.getElementById('account-pill').addEventListener('click', toggleDropdown);
    dom.ddHead.src = src;
    dom.ddName.textContent = state.ign;
    dom.ddUuid.textContent = state.uuid;
  } else {
    dom.accountArea.innerHTML = '<button id="btn-signin" class="btn-signin">Sign in</button>';
    document.getElementById('btn-signin').addEventListener('click', openSigninModal);
  }
  if (window.setUtilityAuthEnabled) window.setUtilityAuthEnabled(!!state.bearerToken);
}

function toggleDropdown() {
  const dd = dom.accountDropdown;
  if (dd.style.display === 'none' || !dd.style.display) {
    const pill = document.getElementById('account-pill');
    const rect = pill.getBoundingClientRect();
    dd.style.display = 'block';
    dd.style.right = (window.innerWidth - rect.right) + 'px';
    dd.style.top = rect.bottom + 4 + 'px';
  } else {
    dd.style.display = 'none';
  }
}

document.addEventListener('click', (e) => {
  if (!dom.accountDropdown.contains(e.target) && !e.target.closest('.account-pill')) {
    dom.accountDropdown.style.display = 'none';
  }
});

function openSigninModal() {
  dom.signinModal.style.display = 'flex';
  dom.deviceCodeArea.style.display = 'none';
  dom.dcSpinner.style.display = 'none';
  dom.btnOpenBrowser.style.display = 'none';
  dom.dcStatus.textContent = '';
  dom.btnAuthStart.disabled = false;
  dom.btnAuthStart.textContent = 'Start Sign In';
  refreshSavedAccountsList();
}

dom.btnCloseModal.addEventListener('click', () => { dom.signinModal.style.display = 'none'; });
dom.signinModal.addEventListener('click', (e) => { if (e.target === dom.signinModal) dom.signinModal.style.display = 'none'; });

dom.btnAuthStart.addEventListener('click', async () => {
  dom.btnAuthStart.disabled = true;
  dom.btnAuthStart.textContent = 'Starting...';
  try {
    const start = await window.__TAURI__.core.invoke('start_auth');
    dom.deviceCodeArea.style.display = 'block';
    dom.dcSpinner.style.display = 'block';
    dom.dcUri.parentElement.style.display = '';
    dom.btnOpenBrowser.style.display = 'inline-block';
    dom.dcStatus.textContent = 'Waiting for you to sign in...';
    dom.dcStatus.style.color = '';
    dom.btnAuthStart.textContent = 'Sign in with Microsoft';
    if (start.flow === 'code') {
      dom.dcCode.style.display = 'none';
      dom.dcUri.textContent = 'Open Microsoft sign-in';
      dom.dcUri.href = start.verification_uri;
      dom.btnOpenBrowser.onclick = () => {
        window.__TAURI__.opener.openUrl(start.verification_uri);
      };
      pollAuthCode();
    } else {
      dom.dcCode.style.display = '';
      dom.dcUri.textContent = start.verification_uri;
      dom.dcUri.href = start.verification_uri;
      dom.dcCode.textContent = start.user_code;
      dom.btnOpenBrowser.onclick = () => {
        window.__TAURI__.opener.openUrl('https://www.microsoft.com/link?otc=' + start.user_code);
      };
      pollDeviceCode(start.device_code, start.interval * 1000);
    }
  } catch (e) {
    dom.dcStatus.textContent = 'Error: ' + e.message;
    dom.dcStatus.style.color = '#FF5555';
    dom.btnAuthStart.textContent = 'Start Sign In';
    dom.btnAuthStart.disabled = false;
    dom.dcSpinner.style.display = 'none';
  }
});

async function completeAuth(result) {
  state.bearerToken = result.bearerToken;
  state.refreshToken = result.refreshToken || null;
  dom.dcSpinner.style.display = 'none';
  dom.btnOpenBrowser.style.display = 'none';
  dom.dcStatus.textContent = 'Signed in!';
  dom.dcStatus.style.color = '#55FF55';
  await fetchProfile();
}

function pollDeviceCode(deviceCode, interval) {
  if (state.pollTimer) clearTimeout(state.pollTimer);
  async function tick() {
    try {
      const result = await window.__TAURI__.core.invoke('poll_auth_token', { deviceCode });
      if (result.status === 'success') {
        await completeAuth(result);
        dom.signinModal.style.display = 'none';
        return;
      }
      if (result.status === 'error') {
        dom.dcSpinner.style.display = 'none';
        dom.dcStatus.textContent = 'Error: ' + result.message;
        dom.dcStatus.style.color = '#FF5555';
        dom.btnAuthStart.disabled = false;
        return;
      }
      if (result.status === 'slow_down') interval = Math.min(interval + 5000, 30000);
      state.pollTimer = setTimeout(tick, interval);
    } catch (e) {
      dom.dcSpinner.style.display = 'none';
      dom.dcStatus.textContent = 'Error: ' + e.message;
      dom.dcStatus.style.color = '#FF5555';
      dom.btnAuthStart.disabled = false;
    }
  }
  state.pollTimer = setTimeout(tick, interval);
}

function pollAuthCode() {
  if (state.pollTimer) clearTimeout(state.pollTimer);
  async function tick() {
    try {
      const result = await window.__TAURI__.core.invoke('poll_auth_code');
      if (result.status === 'success') {
        await completeAuth(result);
        dom.signinModal.style.display = 'none';
        return;
      }
      if (result.status === 'error') {
        dom.dcSpinner.style.display = 'none';
        dom.dcStatus.textContent = 'Error: ' + result.message;
        dom.dcStatus.style.color = '#FF5555';
        dom.btnAuthStart.disabled = false;
        return;
      }
      state.pollTimer = setTimeout(tick, 2000);
    } catch (e) {
      dom.dcSpinner.style.display = 'none';
      dom.dcStatus.textContent = 'Error: ' + e.message;
      dom.dcStatus.style.color = '#FF5555';
      dom.btnAuthStart.disabled = false;
    }
  }
  state.pollTimer = setTimeout(tick, 2000);
}

async function fetchProfile() {
  try {
    const profile = await window.__TAURI__.core.invoke('fetch_profile', { bearerToken: state.bearerToken });
    state.ign = profile.name;
    state.uuid = profile.id;
    await window.__TAURI__.core.invoke('save_account', {
      account: { ign: profile.name, uuid: profile.id, refreshToken: state.refreshToken }
    });
    await updateAccountArea();
    autoDetectOriginalSkin();
  } catch (e) {
    state.ign = null;
    state.uuid = null;
  }
}

async function autoDetectOriginalSkin() {
  if (!state.uuid) return;
  if (state.originalSkinPath) return;
  try {
    const result = await window.__TAURI__.core.invoke('download_skin_texture', { uuid: state.uuid });
    if (result.success) {
      const tmpPath = await window.__TAURI__.core.invoke('save_temp_buffer', {
        data: result.data,
        filename: 'myskinart_original_' + Date.now() + '.png'
      });
      state.originalSkinPath = tmpPath;
      state.originalSkinSource = 'auto';
      dom.originalName.textContent = state.ign + ' (current skin) -> #27';
      dom.originalZone.classList.add('has-file');
      const img = new Image();
      img.src = 'data:image/png;base64,' + result.data;
      img.onload = () => {
        const c8 = document.createElement('canvas');
        c8.width = 64; c8.height = 64;
        const c8Ctx = c8.getContext('2d');
        c8Ctx.imageSmoothingEnabled = false;
        c8Ctx.drawImage(img, 8, 8, 8, 8, 0, 0, 64, 64);
        c8Ctx.drawImage(img, 40, 8, 8, 8, 0, 0, 64, 64);
        state.originalSkinHead = c8.toDataURL();
        updateCell27();
        updatePreviewCell27();
      };
      img.onerror = () => {
        state.originalSkinHead = 'https://mc-heads.net/avatar/' + state.uuid + '/64?t=' + Date.now();
        updateCell27();
        updatePreviewCell27();
      };
    }
  } catch {}
}

// ── Saved Accounts List ──────────────────────────────────────────

async function refreshSavedAccountsList() {
  const accounts = await window.__TAURI__.core.invoke('load_accounts');
  if (!accounts.length) {
    dom.savedAccountsSection.style.display = 'none';
    return;
  }
  dom.savedAccountsSection.style.display = '';
  dom.savedAccountsList.innerHTML = '';
  const avatars = await Promise.all(
    accounts.map(a => window.__TAURI__.core.invoke('fetch_avatar', { id: a.uuid || a.ign }))
  );
  for (let i = 0; i < accounts.length; i++) {
    const acct = accounts[i];
    const row = document.createElement('div');
    row.className = 'saved-account-row';
    const src = avatars[i].success ? avatars[i].dataUrl : '';
    row.innerHTML =
      '<img class="saved-head" src="' + src + '" alt="">' +
      '<span class="saved-name">' + escHtml(acct.ign) + '</span>' +
      '<button class="btn-load-acct" data-ign="' + escHtml(acct.ign) + '">Load</button>' +
      '<button class="btn-remove-acct" data-ign="' + escHtml(acct.ign) + '">&times;</button>';
    row.querySelector('.btn-load-acct').addEventListener('click', async (e) => {
      const ign = e.target.dataset.ign;
      await loadSavedAccount(ign);
    });
    row.querySelector('.btn-remove-acct').addEventListener('click', async (e) => {
      const ign = e.target.dataset.ign;
      await window.__TAURI__.core.invoke('delete_account', { ign });
      if (state.ign === ign) {
        state.bearerToken = null; state.refreshToken = null; state.ign = null; state.uuid = null;
        await updateAccountArea();
      }
      refreshSavedAccountsList();
    });
    dom.savedAccountsList.appendChild(row);
  }
}

async function loadSavedAccount(ign) {
  const accounts = await window.__TAURI__.core.invoke('load_accounts');
  const acct = accounts.find(a => a.ign === ign);
  if (!acct) return;
  if (!acct.refreshToken) {
    dom.dcStatus.textContent = 'No saved token for ' + ign + ' — sign in again to store one.';
    dom.dcStatus.style.color = '#FF5555';
    return;
  }

  dom.dcSpinner.style.display = 'block';
  dom.deviceCodeArea.style.display = 'block';
  dom.btnOpenBrowser.style.display = 'none';
  dom.dcUri.parentElement.style.display = 'none';
  dom.dcCode.style.display = 'none';
  dom.dcStatus.textContent = 'Refreshing token for ' + ign + '...';
  dom.dcStatus.style.color = '';
  try {
    const result = await window.__TAURI__.core.invoke('refresh_saved_token', { refreshToken: acct.refreshToken });
    if (result.success) {
      state.bearerToken = result.bearerToken;
      state.refreshToken = result.refreshToken;
      state.ign = acct.ign;
      state.uuid = acct.uuid || null;
      await window.__TAURI__.core.invoke('save_account', {
        account: { ign: acct.ign, uuid: acct.uuid, refreshToken: result.refreshToken }
      });
      dom.signinModal.style.display = 'none';
      await updateAccountArea();
      autoDetectOriginalSkin();
    } else {
      dom.dcStatus.textContent = 'Refresh failed: ' + result.error;
      dom.dcStatus.style.color = '#FF5555';
      dom.dcSpinner.style.display = 'none';
    }
  } catch (e) {
    dom.dcStatus.textContent = 'Error: ' + e.message;
    dom.dcStatus.style.color = '#FF5555';
    dom.dcSpinner.style.display = 'none';
  }
}

// ── Dropdown Actions ─────────────────────────────────────────────

dom.ddNamemc.addEventListener('click', () => {
  if (state.ign) window.__TAURI__.core.invoke('open_url', { url: 'https://namemc.com/profile/' + state.ign });
  dom.accountDropdown.style.display = 'none';
});

dom.ddSwitch.addEventListener('click', () => {
  dom.accountDropdown.style.display = 'none';
  openSigninModal();
});

dom.ddLogout.addEventListener('click', () => {
  state.bearerToken = null; state.refreshToken = null; state.ign = null; state.uuid = null;
  state.originalSkinPath = null; state.originalSkinHead = null;
  dom.originalName.textContent = 'Defaults to last tile from art';
  dom.originalZone.classList.remove('has-file');
  dom.accountDropdown.style.display = 'none';
  updateAccountArea();
  updateCell27();
  updatePreviewCell27();
});

// ── Upload Flow ──────────────────────────────────────────────────

async function uploadWithRetry(skin, maxRetries) {
  maxRetries = maxRetries || 3;
  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    const result = await window.__TAURI__.core.invoke('upload_one_skin', {
      bearerToken: state.bearerToken, skinPath: skin.path, variant: skin.variant || 'classic'
    });
    if (result.success) return result;
    if (result.statusCode === 401) {
      let rt = state.refreshToken;
      if (!rt && state.ign) {
        try {
          const accounts = await window.__TAURI__.core.invoke('load_accounts');
          const acct = accounts.find(a => a.ign === state.ign);
          if (acct && acct.refreshToken) rt = acct.refreshToken;
        } catch {}
      }
      if (rt) {
        try {
          const refresh = await window.__TAURI__.core.invoke('refresh_saved_token', { refreshToken: rt });
          if (refresh.success) {
            state.bearerToken = refresh.bearerToken;
            state.refreshToken = refresh.refreshToken;
            await window.__TAURI__.core.invoke('save_account', {
              account: { ign: state.ign, uuid: state.uuid, refreshToken: refresh.refreshToken }
            });
            attempt--;
            continue;
          }
        } catch (e) {}
      } else {
        return { success: false, error: 'Re-authentication required', statusCode: 401 };
      }
    }
    if (attempt < maxRetries) {
      await new Promise(r => setTimeout(r, 5000));
    } else {
      return result;
    }
  }
}

function askSkinModel() {
  return new Promise((resolve) => {
    const modal = document.getElementById('model-modal');
    const btnSlim = document.getElementById('btn-model-slim');
    const btnWide = document.getElementById('btn-model-wide');
    modal.style.display = 'flex';
    const pick = (variant) => { modal.style.display = 'none'; resolve(variant); };
    btnSlim.onclick = () => pick('slim');
    btnWide.onclick = () => pick('classic');
  });
}

async function runUpload(startNum) {
  if (state.uploadRunning) return;
  if (!state.bearerToken) { openSigninModal(); return; }
  state.uploadRunning = true;
  dom.btnStart.disabled = true;
  dom.btnStart.textContent = 'Uploading...';

  dom.confirmArea.style.display = '';
  dom.confirmTitle.textContent = 'Starting upload...';
  dom.confirmHint.style.display = 'none';
  dom.pollStatus.className = 'poll-status';
  dom.pollText.textContent = '';
  dom.btnSkipWait.style.display = 'none';

  if (startNum != null) {
    for (const s of state.skins) {
      if (s.num < startNum) updateUploadCell(s.num, 'skip');
    }
  }

  const sorted = state.skins.slice().sort((a, b) => a.num - b.num);
  const toUpload = startNum != null ? sorted.filter(s => s.num >= startNum) : sorted;
  for (const skin of toUpload) {
    highlightGridCell(skin.num);
    dom.confirmTitle.textContent = 'Uploading skin ' + skin.num + '...';
    dom.confirmHint.style.display = 'none';
    dom.pollStatus.className = 'poll-status';
    dom.pollText.textContent = 'Uploading to Minecraft session server...';
    dom.btnSkipWait.style.display = 'none';
    const result = await uploadWithRetry(skin);
    if (!result.success) {
      updateUploadCell(skin.num, 'error');
      dom.confirmTitle.textContent = 'Skin ' + skin.num + ' failed!';
      dom.pollStatus.className = 'poll-status err';
      dom.pollText.textContent = result.error;
      continue;
    }
    updateUploadCell(skin.num, 'uploaded', skin.path);
    if (settings.autoVerify) {
      await waitForNextSkin(skin.num, skin.path);
    } else {
      dom.confirmTitle.textContent = 'Skin ' + skin.num + ' uploaded!';
      dom.pollStatus.className = 'poll-status done';
      dom.pollText.textContent = 'Verification disabled in settings';
    }
    if (settings.uploadDelay > 0) {
      dom.pollStatus.className = 'poll-status';
      dom.pollText.textContent = 'Waiting ' + settings.uploadDelay + 's before next skin...';
      await new Promise(r => setTimeout(r, settings.uploadDelay * 1000));
    }
  }
  if (state.originalSkinPath && state.skinModel) {
    highlightGridCell(27);
    dom.confirmTitle.textContent = 'Uploading skin 27 (original)...';
    dom.confirmHint.style.display = 'none';
    dom.pollStatus.className = 'poll-status';
    dom.pollText.textContent = 'Uploading to Minecraft session server...';
    dom.btnSkipWait.style.display = 'none';
    const result = await uploadWithRetry({ path: state.originalSkinPath, num: 27, variant: state.skinModel });
    if (!result.success) {
      updateUploadCell(27, 'error');
      dom.confirmTitle.textContent = 'Skin 27 (original) failed!';
      dom.pollStatus.className = 'poll-status err';
      dom.pollText.textContent = result.error;
    } else {
      updateUploadCell(27, 'uploaded', state.originalSkinPath);
      await waitForNextSkin(27, state.originalSkinPath, null);
    }
  }
  dom.btnStart.textContent = 'Done!';
  state.uploadRunning = false;
  const failedSkins = state.skins.filter(s => {
    const cell = document.getElementById('cell-' + s.num);
    return cell && cell.classList.contains('failed');
  });
  if (failedSkins.length > 0) {
    dom.confirmTitle.textContent = 'Upload complete';
    dom.pollStatus.className = 'poll-status err';
    dom.pollText.textContent = failedSkins.length + ' skin(s) failed — use Retry Failed below';
    dom.btnStart.textContent = 'Retry Failed (' + failedSkins.length + ')';
    dom.btnStart.onclick = () => {
      dom.btnStart.onclick = () => runUpload(null);
      runUpload(failedSkins[0].num);
    };
  } else {
    switchToDone();
  }
}

dom.btnStart.addEventListener('click', () => runUpload(null));

function highlightGridCell(num) {
  document.querySelectorAll('.upload-grid .cell').forEach(c => c.classList.remove('active-cell'));
  const cell = document.getElementById('cell-' + num);
  if (cell) cell.classList.add('active-cell');
}

function compareFaces(nmcBase64, uploadedBase64) {
  const SIZE = 64;
  return new Promise((resolve) => {
    const nmImg = new Image();
    nmImg.onload = () => {
      const upImg = new Image();
      upImg.onload = () => {
        const nmCanvas = document.createElement('canvas');
        nmCanvas.width = SIZE; nmCanvas.height = SIZE;
        const nmCtx = nmCanvas.getContext('2d');
        nmCtx.imageSmoothingEnabled = false;
        nmCtx.drawImage(nmImg, 0, 0, SIZE, SIZE);
        const nmData = nmCtx.getImageData(0, 0, SIZE, SIZE).data;

        const upCanvas = document.createElement('canvas');
        upCanvas.width = SIZE; upCanvas.height = SIZE;
        const upCtx = upCanvas.getContext('2d');
        upCtx.imageSmoothingEnabled = false;
        upCtx.drawImage(upImg, 0, 0, SIZE, SIZE);
        const upData = upCtx.getImageData(0, 0, SIZE, SIZE).data;

        let matching = 0, compared = 0;
        for (let i = 0; i < nmData.length; i += 4) {
          if (nmData[i + 3] > 10 || upData[i + 3] > 10) {
            compared++;
            const dr = Math.abs(nmData[i] - upData[i]);
            const dg = Math.abs(nmData[i + 1] - upData[i + 1]);
            const db = Math.abs(nmData[i + 2] - upData[i + 2]);
            if (dr < 30 && dg < 30 && db < 30) matching++;
          }
        }
        resolve(compared > 0 ? matching / compared : 0);
      };
      upImg.onerror = () => resolve(0);
      upImg.src = 'data:image/png;base64,' + uploadedBase64;
    };
    nmImg.onerror = () => resolve(0);
    nmImg.src = 'data:image/png;base64,' + nmcBase64;
  });
}

async function waitForNextSkin(num, skinPath) {
  dom.confirmArea.style.display = '';
  dom.confirmTitle.textContent = 'Skin ' + num + ' uploaded!';
  dom.confirmHint.style.display = '';
  dom.confirmHint.textContent = 'Waiting for skin to appear on session server.';
  dom.btnSkipWait.style.display = '';
  dom.pollStatus.className = 'poll-status';
  dom.pollText.textContent = 'Waiting for skin to propagate...';
  if (state.ign) dom.confirmHead.src = 'https://mc-heads.net/head/' + encodeURIComponent(state.ign) + '/128?t=' + Date.now();

  const maxAttempts = 12;
  const interval = 10000;

  return new Promise((resolve) => {
    let resolved = false;
    let attempts = 0;
    let timer = null;

    function finish() {
      if (resolved) return;
      resolved = true;
      if (timer) clearInterval(timer);
      dom.btnSkipWait.style.display = 'none';
      resolve();
    }

    dom.btnSkipWait.onclick = () => finish();

    setTimeout(async () => {
      let uploadedBase64 = null;
      try {
        const fileResult = await window.__TAURI__.core.invoke('read_file_base64', { filePath: skinPath });
        if (fileResult.success) uploadedBase64 = fileResult.data;
      } catch {}

      timer = setInterval(async () => {
        attempts++;
        dom.pollStatus.className = 'poll-status';
        dom.pollText.textContent = 'Checking NameMC... (' + attempts + '/' + maxAttempts + ')';

        try {
          const nmResult = await window.__TAURI__.core.invoke('scrape_namemc_skin', { ign: state.ign });
          if (resolved) return;
          if (nmResult.success && nmResult.skinDataBase64 && uploadedBase64) {
            const ratio = await compareFaces(nmResult.skinDataBase64, uploadedBase64);
            if (ratio >= 1.0) {
              dom.confirmTitle.textContent = 'Skin ' + num + ' verified!';
              dom.pollStatus.className = 'poll-status done';
              dom.pollText.textContent = 'Verified on NameMC!';
              dom.confirmHint.style.display = 'none';
              finish();
              return;
            }
          }
        } catch (e) {
          dom.pollText.textContent = 'Check failed: ' + e.message;
        }

        if (attempts >= maxAttempts) {
          dom.pollStatus.className = 'poll-status done';
          dom.pollText.textContent = 'Timed out — moving to next skin';
          dom.confirmHint.style.display = 'none';
          finish();
        }
      }, interval);
    }, 10000);
  });
}

// ── Export ────────────────────────────────────────────────────────

dom.btnExport.addEventListener('click', async () => {
  dom.btnExport.disabled = true;
  dom.btnExport.textContent = 'Exporting...';
  try {
    const dest = await window.__TAURI__.core.invoke('select_export_dir', { opts: { skins: state.skins } });
    if (dest) {
      dom.btnExport.textContent = 'Exported to ' + dest.split('\\').pop();
      dom.btnExport.disabled = false;
    } else {
      dom.btnExport.textContent = 'Export Skins Only';
      dom.btnExport.disabled = false;
    }
  } catch (e) {
    dom.btnExport.textContent = 'Export failed';
    dom.btnExport.disabled = false;
  }
});

// ── Grid ──────────────────────────────────────────────────────────

function setupGrid() {
  dom.uploadGrid.innerHTML = '';
  const grid = [[27,26,25,24,23,22,21,20,19],[18,17,16,15,14,13,12,11,10],[9,8,7,6,5,4,3,2,1]];
  for (const row of grid) {
    for (const num of row) {
      const cell = document.createElement('div');
      cell.className = 'cell';
      cell.id = 'cell-' + num;
      if (num === 27) {
        const src = state.originalSkinHead || state.lastTileDataUrl;
        if (src) {
          cell.style.backgroundImage = 'url(' + src + ')';
          cell.style.backgroundSize = 'cover';
          cell.textContent = '';
        } else {
          cell.textContent = '27';
        }
      } else {
        cell.textContent = num;
      }
      if (num >= 1 && num <= 26 && !state.uploadRunning) {
        cell.classList.add('clickable');
        cell.addEventListener('click', () => {
          if (state.uploadRunning) return;
          runUpload(num);
        });
      }
      dom.uploadGrid.appendChild(cell);
    }
  }
}

function updateCell27() {
  const cell = document.getElementById('cell-27');
  if (!cell) return;
  const src = state.originalSkinHead || state.lastTileDataUrl || (state.tileData && state.tileData[27]) || null;
  if (src) {
    cell.style.backgroundImage = 'url(' + src + ')';
    cell.style.backgroundSize = 'cover';
    cell.textContent = '';
  } else {
    cell.style.backgroundImage = '';
    cell.textContent = '27';
  }
}

function updateUploadCell(num, status, skinPath) {
  const cell = document.getElementById('cell-' + num);
  if (!cell) return;
  cell.classList.remove('uploaded', 'failed', 'skipped', 'active-cell');
  cell.style.backgroundImage = '';
  cell.style.backgroundSize = '';
  if (status === 'uploaded') {
    cell.classList.add('uploaded');
    if (skinPath) {
      readFileAsDataUrl(skinPath).then(dataUrl => {
        if (dataUrl) {
          cell.style.backgroundImage = 'url(' + dataUrl + ')';
          cell.style.backgroundSize = 'cover';
          cell.style.backgroundPosition = 'top left';
          cell.textContent = '';
        }
      });
    }
  } else if (status === 'error') {
    cell.classList.add('failed');
    cell.textContent = num;
  } else if (status === 'skip') {
    cell.classList.add('skipped');
  }
}

// ── Preview ───────────────────────────────────────────────────────

function renderPreview() {
  if (!state.inputPath) return;
  fetchPreviewTiles(state.inputPath).then(tiles => {
    state.tileData = tiles;
    state.lastTileDataUrl = tiles[27] || null;
    const grid = [[27,26,25,24,23,22,21,20,19],[18,17,16,15,14,13,12,11,10],[9,8,7,6,5,4,3,2,1]];
    let html = '<div class="preview-grid">';
    for (const row of grid) {
      for (const num of row) {
        let src, label, cls;
        if (num === 27) {
          src = state.originalSkinHead || tiles[27];
          label = state.originalSkinHead ? 'FACE' : '27';
          cls = '';
        } else {
          src = tiles[num];
          label = num;
          cls = '';
        }
        html += '<div class="preview-cell"><img src="' + (src || '') + '" alt="tile ' + num + '"><span class="pl ' + cls + '">' + label + '</span></div>';
      }
    }
    html += '</div>';
    dom.previewWrapper.innerHTML = html;
    dom.previewWrapper.style.display = 'block';
    updatePreviewCell27();
    updateCell27();
  }).catch(() => {});
}

async function fetchPreviewTiles(filePath) {
  const dataUrl = await readFileAsDataUrl(filePath);
  if (!dataUrl) throw new Error('Could not read image: ' + filePath);
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const scaleCanvas = document.createElement('canvas');
      scaleCanvas.width = 72;
      scaleCanvas.height = 24;
      const scaleCtx = scaleCanvas.getContext('2d');
      scaleCtx.imageSmoothingEnabled = false;
      scaleCtx.drawImage(img, 0, 0, 72, 24);
      const map = {};
      for (let row = 0; row < 3; row++) {
        for (let col = 0; col < 9; col++) {
          const num = 27 - (row * 9 + col);
          const c = document.createElement('canvas');
          c.width = 8; c.height = 8;
          const cCtx = c.getContext('2d');
          cCtx.imageSmoothingEnabled = false;
          cCtx.drawImage(scaleCanvas, col * 8, row * 8, 8, 8, 0, 0, 8, 8);
          map[num] = c.toDataURL();
        }
      }
      resolve(map);
    };
    img.onerror = () => reject(new Error('Could not decode image'));
    img.src = dataUrl;
  });
}

function switchToDone() {
  dom.stepUpload.classList.remove('active');
  dom.stepDone.classList.add('active');
  const namemcUrl = state.ign ? 'https://namemc.com/profile/' + state.ign : null;
  dom.stepDone.innerHTML =
    '<div class="done-icon">' +
      '<svg width="80" height="80" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><polyline points="16 8 10.5 14 8 11.5"/></svg>' +
    '</div>' +
    '<div class="done-content">' +
      '<h2>All done!</h2>' +
      '<p>All skins have been uploaded and confirmed.</p>' +
      '<div class="done-actions">' +
        (namemcUrl ? '<a class="btn-namemc" href="' + namemcUrl + '" id="done-namemc-link">View on NameMC</a>' : '') +
        '<button class="btn-restart" id="btn-restart-done">Make another</button>' +
      '</div>' +
    '</div>';
  const namemcLink = document.getElementById('done-namemc-link');
  if (namemcLink) {
    namemcLink.addEventListener('click', (e) => {
      e.preventDefault();
      window.__TAURI__.core.invoke('open_url', { url: namemcUrl });
    });
  }
  document.getElementById('btn-restart-done').addEventListener('click', () => dom.btnRestart.click());
}

// ── Restart ──────────────────────────────────────────────────────

dom.btnRestart.addEventListener('click', async () => {
  if (state.pollTimer) clearTimeout(state.pollTimer);
  if (state.claiming) window.__TAURI__.core.invoke('cancel_claim');
  const savedIgn = state.ign;
  const savedUuid = state.uuid;
  const savedBearer = state.bearerToken;
  const savedRefresh = state.refreshToken;
  state = {
    inputPath: null, baseSkinPath: null, originalSkinPath: null, originalSkinHead: null,
    lastTileDataUrl: null, skins: [], bearerToken: savedBearer, refreshToken: savedRefresh,
    ign: savedIgn, uuid: savedUuid, pollTimer: null, uploadRunning: false, claiming: false,
    templates: state.templates, tileData: {}, skinModel: null, originalSkinSource: null,
    usedTemplate: null,
  };
  dom.inputName.textContent = 'No image selected';
  dom.inputZone.classList.remove('has-file');
  dom.baseName.textContent = 'None';
  dom.baseZone.classList.remove('has-file');
  dom.originalName.textContent = 'Defaults to last tile from art';
  dom.originalZone.classList.remove('has-file');
  dom.btnGenerate.disabled = true;
  dom.previewWrapper.style.display = 'none';
  dom.previewWrapper.innerHTML = '';
  dom.uploadGrid.innerHTML = '';
  dom.confirmArea.style.display = 'none';
  if (templateSearchInput) templateSearchInput.value = '';
  if (templateTagSelect) templateTagSelect.value = '';
  dom.btnStart.disabled = true;
  dom.btnStart.textContent = 'Start Upload';
  dom.btnExport.disabled = true;
  dom.btnExport.textContent = 'Export Skins Only';
  dom.stepDone.classList.remove('active');
  dom.stepUpload.classList.remove('active');
  document.getElementById('tab-create').classList.add('active');
  document.querySelector('.tab-buttons').style.display = '';
  document.querySelector('[data-tab="create"]').click();
  updateAccountArea();
  renderTemplates();
});

// ── Init ──────────────────────────────────────────────────────────

updateAccountArea();
refreshSavedAccountsList();
loadTemplates();
checkForUpdates();
setVersionInfo();
window.__TAURI__.event.listen('templates-updated', () => {
  loadTemplates();
});

// ── Version Info ─────────────────────────────────────────────────

async function setVersionInfo() {
  try {
    const version = await window.__TAURI__.core.invoke('get_app_version');
    const el = document.getElementById('info-version');
    if (el) el.textContent = 'Version ' + version;
    document.title = 'Skinart+ v' + version;
  } catch (e) {
    console.warn('Could not load version info:', e);
  }
}

// ── Update Check ─────────────────────────────────────────────────

async function checkForUpdates() {
  try {
    const info = await window.__TAURI__.core.invoke('check_for_update');
    if (!info || !info.is_outdated) return;
    const banner = document.getElementById('update-banner');
    const text = document.getElementById('update-banner-text');
    if (!banner || !text) return;
    text.textContent = 'A new version (v' + info.latest_version + ') is available. You are on v' + info.current_version + '.';
    banner.style.display = 'flex';
  } catch (e) {
    console.warn('Update check failed:', e);
  }
}

(function() {
  const downloadBtn = document.getElementById('btn-update-download');
  const dismissBtn = document.getElementById('btn-update-dismiss');
  if (downloadBtn) {
    downloadBtn.addEventListener('click', async () => {
      try {
        const platform = await window.__TAURI__.core.invoke('get_os_platform');
        if (platform === 'windows') {
          downloadBtn.textContent = 'Downloading...';
          downloadBtn.disabled = true;
          const dl = await window.__TAURI__.core.invoke('download_update');
          downloadBtn.textContent = 'Installing...';
          await window.__TAURI__.core.invoke('run_update_installer', { path: dl.path });
        } else {
          await window.__TAURI__.core.invoke('open_latest_release');
        }
      } catch (e) {
        console.warn('Update install failed:', e);
        downloadBtn.textContent = 'Download';
        downloadBtn.disabled = false;
      }
    });
  }
  if (dismissBtn) {
    dismissBtn.addEventListener('click', () => {
      const banner = document.getElementById('update-banner');
      if (banner) banner.style.display = 'none';
    });
  }
})();

// ── Design Tab (Pixel Art Canvas) ────────────────────────────────

(function() {
  const designCanvas = document.getElementById('design-canvas');
  const designCtx = designCanvas.getContext('2d', { willReadFrequently: true });
  const designGridCanvas = document.getElementById('design-grid-canvas');
  const designTextPreview = document.getElementById('design-text-preview');
  const designTextPreviewCtx = designTextPreview.getContext('2d');
  const designPlaceholder = document.getElementById('design-placeholder');

  const designColors = [
    '#1a1a2e','#16213e','#0f3460','#533483','#e94560',
    '#ff6b6b','#feca57','#48dbfb','#ff9ff3','#54a0ff',
    '#5f27cd','#01a3a4','#f368e0','#ff9f43','#10ac84',
    '#ee5a24','#0abde3','#8395a7','#c8d6e5','#576574',
    '#222f3e','#1dd1a1','#feca57','#ff6348','#7bed9f',
    '#e056fd','#686de0','#30336b','#130f40','#535c68'
  ];

  let designState = {
    color: '#000000',
    eraseMode: false,
    fillMode: false,
    gridOn: false,
    textMode: false,
    textPlacementMode: false,
    textFont: 'Minecraft',
    textSize: 8,
    textColor: '#ffffff',
    undoStack: [],
    redoStack: [],
    drawing: false,
    hasDrawn: false
  };

  function designRenderPalette() {
    const pal = document.getElementById('design-palette');
    pal.innerHTML = '';
    designColors.forEach(c => {
      const sw = document.createElement('div');
      sw.className = 'design-swatch' + (c === designState.color ? ' selected' : '');
      sw.style.background = c;
      sw.title = c;
      sw.onclick = () => {
        designState.color = c;
        document.getElementById('design-color-input').value = c;
        designRenderPalette();
      };
      pal.appendChild(sw);
    });
  }

  function designPushUndo() {
    designState.undoStack.push(designCanvas.toDataURL());
    if (designState.undoStack.length > 50) designState.undoStack.shift();
    designState.redoStack = [];
  }

  function designUndo() {
    if (!designState.undoStack.length) return;
    designState.redoStack.push(designCanvas.toDataURL());
    const img = new Image();
    img.onload = () => {
      designCtx.clearRect(0, 0, 72, 24);
      designCtx.drawImage(img, 0, 0);
    };
    img.src = designState.undoStack.pop();
  }

  function designRedo() {
    if (!designState.redoStack.length) return;
    designState.undoStack.push(designCanvas.toDataURL());
    const img = new Image();
    img.onload = () => {
      designCtx.clearRect(0, 0, 72, 24);
      designCtx.drawImage(img, 0, 0);
    };
    img.src = designState.redoStack.pop();
  }

  function designFloodFill(startX, startY, targetColor, fillColor) {
    const imageData = designCtx.getImageData(0, 0, 72, 24);
    const data = imageData.data;
    const w = 72, h = 24;
    const idx = (startY * w + startX) * 4;
    const t = [data[idx], data[idx+1], data[idx+2], data[idx+3]];
    if (t[0] === fillColor[0] && t[1] === fillColor[1] && t[2] === fillColor[2] && t[3] === fillColor[3]) return;
    const stack = [[startX, startY]];
    const visited = new Set();
    while (stack.length) {
      const [x, y] = stack.pop();
      if (x < 0 || y < 0 || x >= w || y >= h) continue;
      const key = y * w + x;
      if (visited.has(key)) continue;
      visited.add(key);
      const i = key * 4;
      if (Math.abs(data[i]-t[0])>10 || Math.abs(data[i+1]-t[1])>10 || Math.abs(data[i+2]-t[2])>10 || Math.abs(data[i+3]-t[3])>10) continue;
      data[i] = fillColor[0]; data[i+1] = fillColor[1]; data[i+2] = fillColor[2]; data[i+3] = fillColor[3];
      stack.push([x+1,y],[x-1,y],[x,y+1],[x,y-1]);
    }
    designCtx.putImageData(imageData, 0, 0);
  }

  function designDrawPixel(e) {
    if (designState.textPlacementMode) return;
    const rect = designCanvas.getBoundingClientRect();
    const scaleX = 72 / rect.width;
    const scaleY = 24 / rect.height;
    const x = Math.floor((e.clientX - rect.left) * scaleX);
    const y = Math.floor((e.clientY - rect.top) * scaleY);
    if (x < 0 || y < 0 || x >= 72 || y >= 24) return;
    designPushUndo();
    if (designState.fillMode) {
      const hex = designState.color.replace('#','');
      const r = parseInt(hex.substring(0,2),16);
      const g = parseInt(hex.substring(2,4),16);
      const b = parseInt(hex.substring(4,6),16);
      designFloodFill(x, y, [0,0,0,0], [r,g,b,255]);
      designSaveToStorage();
      return;
    }
    designCtx.imageSmoothingEnabled = false;
    designCtx.fillStyle = designState.eraseMode ? 'rgba(0,0,0,0)' : designState.color;
    if (designState.eraseMode) {
      designCtx.clearRect(x, y, 1, 1);
    } else {
      designCtx.fillRect(x, y, 1, 1);
    }
    designSaveToStorage();
    designState.hasDrawn = true;
  }

  function designSaveToStorage() {
    try {
      localStorage.setItem('myskinart-design', designCanvas.toDataURL());
    } catch {}
  }

  function designLoadFromStorage() {
    try {
      const data = localStorage.getItem('myskinart-design');
      if (!data) return;
      const img = new Image();
      img.onload = () => {
        designCtx.clearRect(0, 0, 72, 24);
        designCtx.drawImage(img, 0, 0);
        designPlaceholder.style.display = 'none';
      };
      img.src = data;
    } catch {}
  }

  function designDrawGrid() {
    const gctx = designGridCanvas.getContext('2d');
    gctx.clearRect(0, 0, 72, 24);
    if (designState.gridOn) {
      for (let by = 0; by < 3; by++) {
        for (let bx = 0; bx < 9; bx++) {
          if ((bx + by) % 2 === 0) {
            gctx.fillStyle = 'rgba(200,200,200,0.18)';
            gctx.fillRect(bx * 8, by * 8, 8, 8);
          }
        }
      }
    }
    designGridCanvas.style.display = designState.gridOn ? 'block' : 'none';
  }

  function designPlaceText(clickX, clickY) {
    const text = document.getElementById('design-text-input').value;
    if (!text) return;
    const font = document.getElementById('design-font-select').value;
    const size = parseInt(document.getElementById('design-font-size').value);
    designPushUndo();
    designCtx.imageSmoothingEnabled = false;
    designCtx.font = size + 'px ' + font;
    designCtx.fillStyle = designState.textColor;
    designCtx.textBaseline = 'top';
    const measure = designCtx.measureText(text);
    const textW = Math.ceil(measure.width);
    const textH = size;
    const startX = Math.max(0, Math.min(72 - textW, clickX));
    const startY = Math.max(0, Math.min(24 - textH, clickY));
    designCtx.fillText(text, startX, startY);
    designState.textPlacementMode = false;
    designCanvas.style.cursor = 'crosshair';
    document.getElementById('design-text-btn').classList.remove('selected');
    document.getElementById('design-text-info').textContent = 'Text placed! (' + textW + 'x' + textH + ')';
    designSaveToStorage();
    designHideTextPreview();
  }

  function designRenderTextPreview(px, py) {
    const text = document.getElementById('design-text-input').value;
    if (!text) { designTextPreview.style.display = 'none'; return; }
    const font = document.getElementById('design-font-select').value;
    const size = parseInt(document.getElementById('design-font-size').value);
    designTextPreviewCtx.clearRect(0, 0, 72, 24);
    designTextPreviewCtx.imageSmoothingEnabled = false;
    designTextPreviewCtx.font = size + 'px ' + font;
    designTextPreviewCtx.fillStyle = designState.textColor;
    designTextPreviewCtx.textBaseline = 'top';
    const measure = designTextPreviewCtx.measureText(text);
    const textW = Math.ceil(measure.width);
    const textH = size;
    const startX = Math.max(0, Math.min(72 - textW, px));
    const startY = Math.max(0, Math.min(24 - textH, py));
    designTextPreviewCtx.fillText(text, startX, startY);
    designTextPreview.style.display = 'block';
  }

  function designHideTextPreview() {
    designTextPreviewCtx.clearRect(0, 0, 72, 24);
    designTextPreview.style.display = 'none';
  }

  function designExportPNG() {
    const c = document.createElement('canvas');
    c.width = 72; c.height = 24;
    const ctx2 = c.getContext('2d');
    ctx2.drawImage(designCanvas, 0, 0);
    const link = document.createElement('a');
    link.download = 'myskinart_design.png';
    link.href = c.toDataURL('image/png');
    link.click();
  }

  function designImportImage(file) {
    const reader = new FileReader();
    reader.onload = (e) => {
      const img = new Image();
      img.onload = () => {
        designPushUndo();
        designCtx.clearRect(0, 0, 72, 24);
        designCtx.imageSmoothingEnabled = false;
        designCtx.drawImage(img, 0, 0, 72, 24);
        designPlaceholder.style.display = 'none';
        designSaveToStorage();
      };
      img.src = e.target.result;
    };
    reader.readAsDataURL(file);
  }

  function designUseInCreate() {
    const dataUrl = designCanvas.toDataURL('image/png');
    const base64 = dataUrl.split(',')[1];
    window.__TAURI__.core.invoke('save_temp_buffer', { data: base64, filename: 'myskinart_design_' + Date.now() + '.png' }).then(tmpPath => {
      state.inputPath = tmpPath;
      state.usedTemplate = null;
      dom.inputName.textContent = 'Design canvas';
      dom.inputZone.classList.add('has-file');
      updateGenerateBtn();
      renderPreview();
      document.querySelector('[data-tab="create"]').click();
    });
  }

  designCanvas.addEventListener('mousedown', (e) => {
    if (designState.textPlacementMode) {
      const rect = designCanvas.getBoundingClientRect();
      const scaleX = 72 / rect.width;
      const scaleY = 24 / rect.height;
      const px = Math.floor((e.clientX - rect.left) * scaleX);
      const py = Math.floor((e.clientY - rect.top) * scaleY);
      designPlaceText(px, py);
      return;
    }
    designState.drawing = true;
    designDrawPixel(e);
  });
  designCanvas.addEventListener('mousemove', (e) => {
    if (designState.textPlacementMode) {
      const rect = designCanvas.getBoundingClientRect();
      const scaleX = 72 / rect.width;
      const scaleY = 24 / rect.height;
      const px = Math.floor((e.clientX - rect.left) * scaleX);
      const py = Math.floor((e.clientY - rect.top) * scaleY);
      designRenderTextPreview(px, py);
      return;
    }
    if (designState.drawing) designDrawPixel(e);
  });
  designCanvas.addEventListener('mouseup', () => { designState.drawing = false; });
  designCanvas.addEventListener('mouseleave', () => {
    designState.drawing = false;
    if (designState.textPlacementMode) designHideTextPreview();
  });

  document.getElementById('design-color-input').addEventListener('input', (e) => {
    if (/^#[0-9a-fA-F]{6}$/.test(e.target.value)) {
      designState.color = e.target.value;
      designRenderPalette();
    }
  });

  document.getElementById('design-erase-btn').addEventListener('click', function() {
    designState.eraseMode = !designState.eraseMode;
    this.classList.toggle('selected', designState.eraseMode);
    if (designState.eraseMode) designState.fillMode = false;
    document.getElementById('design-fill-btn').classList.remove('selected');
  });

  document.getElementById('design-fill-btn').addEventListener('click', function() {
    designState.fillMode = !designState.fillMode;
    this.classList.toggle('selected', designState.fillMode);
    if (designState.fillMode) designState.eraseMode = false;
    document.getElementById('design-erase-btn').classList.remove('selected');
  });

  document.getElementById('design-grid-toggle').addEventListener('click', function() {
    designState.gridOn = !designState.gridOn;
    this.classList.toggle('selected', designState.gridOn);
    designDrawGrid();
  });

  document.getElementById('design-clear-btn').addEventListener('click', () => {
    if (!confirm('Clear the canvas?')) return;
    designPushUndo();
    designCtx.clearRect(0, 0, 72, 24);
    designPlaceholder.style.display = 'flex';
    designSaveToStorage();
  });

  document.getElementById('design-undo').addEventListener('click', designUndo);
  document.getElementById('design-redo').addEventListener('click', designRedo);

  document.getElementById('design-text-btn').addEventListener('click', function() {
    designState.textPlacementMode = !designState.textPlacementMode;
    this.classList.toggle('selected', designState.textPlacementMode);
    designCanvas.style.cursor = designState.textPlacementMode ? 'cell' : 'crosshair';
    if (designState.textPlacementMode) {
      document.getElementById('design-text-info').textContent = 'Click on canvas to place text';
      designRenderTextPreview(0, 0);
    } else {
      document.getElementById('design-text-info').textContent = '';
      designHideTextPreview();
    }
  });

  document.getElementById('design-text-color').style.background = '#ffffff';
  document.getElementById('design-text-color').addEventListener('click', function() {
    const input = document.createElement('input');
    input.type = 'color';
    input.value = designState.textColor;
    input.addEventListener('input', (e) => {
      designState.textColor = e.target.value;
      document.getElementById('design-text-color').style.background = e.target.value;
      if (designState.textPlacementMode) designRenderTextPreview(0, 0);
    });
    input.click();
  });

  function designRefreshPreviewIfActive() {
    if (designState.textPlacementMode) designRenderTextPreview(0, 0);
  }

  document.getElementById('design-text-input').addEventListener('input', designRefreshPreviewIfActive);
  document.getElementById('design-font-select').addEventListener('change', designRefreshPreviewIfActive);
  document.getElementById('design-font-size').addEventListener('change', designRefreshPreviewIfActive);

  document.getElementById('design-import').addEventListener('click', () => {
    document.getElementById('design-import-input').click();
  });
  document.getElementById('design-import-input').addEventListener('change', (e) => {
    if (e.target.files[0]) designImportImage(e.target.files[0]);
    e.target.value = '';
  });

  document.getElementById('design-download').addEventListener('click', designExportPNG);
  document.getElementById('design-use').addEventListener('click', designUseInCreate);

  document.addEventListener('keydown', (e) => {
    const activeTab = document.querySelector('.tab-content#tab-design');
    if (!activeTab || !activeTab.classList.contains('active')) return;
    if (e.ctrlKey && e.key === 'z') { e.preventDefault(); designUndo(); }
    if (e.ctrlKey && e.key === 'y') { e.preventDefault(); designRedo(); }
  });

  designRenderPalette();
  designLoadFromStorage();
})();

// ── Utility Tab ──────────────────────────────────────────────────

(function() {
  const claimGoBtn = document.getElementById('utility-claim-go');
  const claimStatus = document.getElementById('utility-claim-status');
  const claimText = document.getElementById('utility-claim-text');
  const claimResult = document.getElementById('utility-claim-result');

  function setUtilityAuthEnabled(loggedIn) {
    claimGoBtn.disabled = !loggedIn;
    if (!loggedIn) {
      claimStatus.style.display = 'none';
      claimResult.style.display = 'none';
    }
  }

  window.setUtilityAuthEnabled = setUtilityAuthEnabled;
  setUtilityAuthEnabled(!!state.bearerToken);

  if (window.__TAURI__?.event) {
    window.__TAURI__.event.listen('claim-status', (e) => {
      const msg = (e && e.payload && e.payload.message) || '';
      if (msg && claimText) claimText.textContent = msg;
    });
  }

  let claiming = false;

  claimGoBtn.addEventListener('click', async () => {
    if (claiming || !state.bearerToken || !state.ign) return;
    claiming = true;
    claimGoBtn.disabled = true;
    claimGoBtn.textContent = 'Connecting...';
    claimStatus.style.display = 'flex';
    claimResult.style.display = 'none';
    claimText.textContent = 'Connecting to server...';

    const server = 'blockmania.com';
    const port = 25565;

    try {
      const result = await window.__TAURI__.core.invoke('claim_namemc', {
        request: {
          bearerToken: state.bearerToken,
          profile: { name: state.ign, id: state.uuid || state.ign },
          server, port
        }
      });
      claimStatus.style.display = 'none';
      if (result.success) {
        claimResult.style.display = 'block';
        const url = result.url || '';
        claimResult.innerHTML = '<span class="claim-ok">Claimed!</span> <a href="#" id="utility-claim-link">' + url + '</a>';
        document.getElementById('utility-claim-link').addEventListener('click', (e) => {
          e.preventDefault();
          window.__TAURI__.core.invoke('open_url', { url });
        });
      } else {
        claimResult.style.display = 'block';
        claimResult.innerHTML = '<span class="claim-err">' + (result.error || 'Claim failed') + '</span>';
      }
    } catch (e) {
      claimStatus.style.display = 'none';
      claimResult.style.display = 'block';
      const msg = (e && e.message) || String(e) || 'Unknown error';
      claimResult.innerHTML = '<span class="claim-err">' + msg + '</span>';
    }

    claimGoBtn.disabled = false;
    claimGoBtn.textContent = 'Claim';
    claiming = false;
  });

  const stealIgn = document.getElementById('utility-steal-ign');
  const stealBtn = document.getElementById('utility-steal-start');
  const stealStatus = document.getElementById('utility-steal-status');
  const stealText = document.getElementById('utility-steal-text');
  const stealResult = document.getElementById('utility-steal-result');
  const stealCanvas = document.getElementById('utility-steal-canvas');
  const stealDownload = document.getElementById('utility-steal-download');
  const stealUse = document.getElementById('utility-steal-use');
  const stealLog = document.getElementById('utility-steal-log');
  let stolenSkinDataUrls = [];

  function stealLogEntry(cls, text) {
    const entry = document.createElement('div');
    entry.className = 'entry ' + cls;
    entry.textContent = text;
    stealLog.prepend(entry);
  }

  stealBtn.addEventListener('click', async () => {
    const ign = stealIgn.value.trim();
    if (!ign) return;

    stealBtn.disabled = true;
    stealBtn.textContent = 'Extracting...';
    stealResult.style.display = 'none';
    stealLog.innerHTML = '';
    stealLog.style.display = 'none';
    stealStatus.style.display = 'flex';
    stealText.textContent = 'Loading NameMC profile for ' + ign + '...';

    try {
      const result = await window.__TAURI__.core.invoke('scrape_namemc_all_skins', { ign });

      if (!result.success) {
        stealText.textContent = 'Error: ' + result.error;
        stealLogEntry('err', 'Failed: ' + result.error);
        stealBtn.disabled = false;
        stealBtn.textContent = 'Steal Skinart';
        return;
      }

      stealText.textContent = 'Extracted ' + result.count + ' skins! Building grid...';

      stolenSkinDataUrls = result.skins;
      await buildStealGrid(result.skins);

      stealStatus.style.display = 'none';
      stealResult.style.display = 'block';
      stealLog.style.display = 'block';
    } catch (e) {
      stealText.textContent = 'Error: ' + e.message;
      stealLogEntry('err', e.message);
    }

    stealBtn.disabled = false;
    stealBtn.textContent = 'Steal Skinart';
  });

  stealIgn.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') stealBtn.click();
  });

  async function buildStealGrid(dataUrls) {
    const TILE = 32;
    const COLS = 9;
    const ROWS = 3;
    const ctx = stealCanvas.getContext('2d');
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, 288, 96);

    const loadImg = (src) => new Promise((resolve) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = () => resolve(null);
      img.src = src;
    });

    for (let i = 0; i < Math.min(dataUrls.length, 27); i++) {
      const img = await loadImg(dataUrls[i]);
      if (!img) continue;
      const col = i % COLS;
      const row = Math.floor(i / COLS);
      ctx.drawImage(img, col * TILE, row * TILE, TILE, TILE);
    }
  }

  stealDownload.addEventListener('click', () => {
    const link = document.createElement('a');
    link.download = 'stolen_skinart_' + (stealIgn.value.trim() || 'unknown') + '.png';
    link.href = stealCanvas.toDataURL('image/png');
    link.click();
  });

  stealUse.addEventListener('click', async () => {
    const dataUrl = stealCanvas.toDataURL('image/png');
    const base64 = dataUrl.split(',')[1];
    const tmpPath = await window.__TAURI__.core.invoke('save_temp_buffer', {
      data: base64,
      filename: 'stolen_skinart_' + Date.now() + '.png'
    });
    state.inputPath = tmpPath;
    state.usedTemplate = null;
    dom.inputName.textContent = 'Stolen: ' + stealIgn.value.trim();
    dom.inputZone.classList.add('has-file');
    updateGenerateBtn();
    renderPreview();
    document.querySelector('[data-tab="create"]').click();
  });
})();

// ── Settings ────────────────────────────────────────────────────

const defaultSettings = {
  theme: 'midnight',
  guiScale: 100,
  uploadDelay: 4,
  autoVerify: true,
  skinNumbers: true
};

const THEMES = [
  { id: 'midnight',    label: 'Default',   bg: '#0f172a', accent: '#14b8a6' },
  { id: 'matte-black', label: 'Onyx',      bg: '#0a0a0a', accent: '#ffffff' },
  { id: 'matte-white', label: 'Pearl',     bg: '#f5f5f5', accent: '#000000' },
  { id: 'matcha',      label: 'Matcha',     bg: '#f2f4e9', accent: '#5c7f2a' },
  { id: 'amethyst',    label: 'Amethyst',   bg: '#181229', accent: '#8b5cf6' },
  { id: 'forest',      label: 'Forest',     bg: '#0b140e', accent: '#22c55e' },
  { id: 'sunset',      label: 'Sunset',     bg: '#17110b', accent: '#f97316' },
  { id: 'blood-moon',  label: 'Blood Moon', bg: '#170a0a', accent: '#dc2626' },
  { id: 'catppuccin',  label: 'Catppuccin', bg: '#eff1f5', accent: '#8839ef' }
];

function loadSettings() {
  try {
    const raw = localStorage.getItem('myskinart-settings');
    if (raw) return { ...defaultSettings, ...JSON.parse(raw) };
  } catch {}
  // Migrate old gui scale key
  try {
    const old = localStorage.getItem('myskinart-gui-scale');
    if (old) {
      const s = { ...defaultSettings, guiScale: Math.round(parseFloat(old) * 100) };
      localStorage.removeItem('myskinart-gui-scale');
      return s;
    }
  } catch {}
  return { ...defaultSettings };
}

function saveSettings(s) {
  try { localStorage.setItem('myskinart-settings', JSON.stringify(s)); } catch {}
}

let settings = loadSettings();

function applySettings() {
  document.body.dataset.theme = settings.theme;
  const themePicker = document.getElementById('settings-theme');
  if (themePicker) {
    Array.from(themePicker.querySelectorAll('.theme-option')).forEach((btn) => {
      btn.classList.toggle('active', btn.dataset.theme === settings.theme);
    });
  }
  document.body.style.zoom = (settings.guiScale / 100).toString();
  const scaleSlider = document.getElementById('settings-gui-scale');
  const scaleVal = document.getElementById('settings-gui-scale-val');
  if (scaleSlider) scaleSlider.value = settings.guiScale;
  if (scaleVal) scaleVal.textContent = settings.guiScale + '%';

  const delaySlider = document.getElementById('settings-upload-delay');
  const delayVal = document.getElementById('settings-upload-delay-val');
  const delayRec = document.getElementById('settings-upload-delay-rec');
  if (delaySlider) delaySlider.value = settings.uploadDelay;
  if (delayVal) delayVal.textContent = settings.uploadDelay + 's';
  if (delayRec) {
    if (settings.uploadDelay >= 4) {
      delayRec.textContent = '(recommended)';
      delayRec.style.color = '';
    } else {
      delayRec.textContent = '(not recommended)';
      delayRec.style.color = 'var(--warning)';
    }
  }

  const autoVerify = document.getElementById('settings-auto-verify');
  if (autoVerify) autoVerify.checked = settings.autoVerify;

  const skinNumbers = document.getElementById('settings-skin-numbers');
  if (skinNumbers) skinNumbers.checked = settings.skinNumbers;
}

applySettings();

// Settings modal
document.getElementById('btn-settings').addEventListener('click', () => {
  document.getElementById('settings-modal').style.display = 'flex';
  applySettings();
});

document.getElementById('btn-close-settings').addEventListener('click', () => {
  document.getElementById('settings-modal').style.display = 'none';
});

document.getElementById('settings-modal').addEventListener('click', (e) => {
  if (e.target.id === 'settings-modal') e.target.style.display = 'none';
});

// Info modal
document.getElementById('btn-info').addEventListener('click', () => {
  document.getElementById('info-modal').style.display = 'flex';
});

document.getElementById('btn-close-info').addEventListener('click', () => {
  document.getElementById('info-modal').style.display = 'none';
});

document.getElementById('info-modal').addEventListener('click', (e) => {
  if (e.target.id === 'info-modal') e.target.style.display = 'none';
});

document.querySelectorAll('#info-modal .info-link').forEach((link) => {
  link.addEventListener('click', (e) => {
    e.preventDefault();
    window.__TAURI__.core.invoke('open_url', { url: link.dataset.url });
  });
});

document.getElementById('settings-gui-scale').addEventListener('input', (e) => {
  settings.guiScale = parseInt(e.target.value);
  document.getElementById('settings-gui-scale-val').textContent = settings.guiScale + '%';
  applySettings();
  saveSettings(settings);
});

document.getElementById('settings-upload-delay').addEventListener('input', (e) => {
  settings.uploadDelay = parseInt(e.target.value);
  document.getElementById('settings-upload-delay-val').textContent = settings.uploadDelay + 's';
  const rec = document.getElementById('settings-upload-delay-rec');
  if (settings.uploadDelay >= 4) {
    rec.textContent = '(recommended)';
    rec.style.color = '';
  } else {
    rec.textContent = '(not recommended)';
    rec.style.color = 'var(--warning)';
  }
  saveSettings(settings);
});

document.getElementById('settings-auto-verify').addEventListener('change', (e) => {
  settings.autoVerify = e.target.checked;
  saveSettings(settings);
});

document.getElementById('settings-skin-numbers').addEventListener('change', (e) => {
  settings.skinNumbers = e.target.checked;
  saveSettings(settings);
});

// Theme picker
(function buildThemePicker() {
  const picker = document.getElementById('settings-theme');
  if (!picker) return;
  THEMES.forEach((t) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'theme-option';
    btn.dataset.theme = t.id;
    btn.title = t.label;
    btn.innerHTML =
      '<span class="theme-swatch" style="background:' + t.bg + '">' +
        '<span class="theme-swatch-accent" style="background:' + t.accent + '"></span>' +
      '</span>' +
      '<span class="theme-name">' + t.label + '</span>';
    btn.addEventListener('click', () => {
      settings.theme = t.id;
      applySettings();
      saveSettings(settings);
    });
    picker.appendChild(btn);
  });
  applySettings();
})();

// Ctrl+scroll / Ctrl+/- zoom also updates settings
(function() {
  document.addEventListener('wheel', (e) => {
    if (e.ctrlKey) {
      e.preventDefault();
      settings.guiScale = Math.max(60, Math.min(200, settings.guiScale + (e.deltaY < 0 ? 5 : -5)));
      applySettings();
      saveSettings(settings);
    }
  }, { passive: false });

  document.addEventListener('keydown', (e) => {
    if (e.ctrlKey && (e.key === '-' || e.key === '_')) {
      e.preventDefault();
      settings.guiScale = Math.max(60, Math.min(200, settings.guiScale - 5));
      applySettings();
      saveSettings(settings);
    }
    if (e.ctrlKey && (e.key === '=' || e.key === '+')) {
      e.preventDefault();
      settings.guiScale = Math.max(60, Math.min(200, settings.guiScale + 5));
      applySettings();
      saveSettings(settings);
    }
  });
})();
