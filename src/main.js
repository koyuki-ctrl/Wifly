import Alpine from 'alpinejs';
import * as lucide from 'lucide';
import QRCode from 'qrcode';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';

const appWindow = getCurrentWindow();

window.Alpine = Alpine;

function wiflyApp() {
  return {
    page: 'dashboard',
    /* Server state */
    running: false,
    loading: false,
    serverIp: null,
    serverPort: null,
    ssid: 'Wifly',
    password: 'wifly1234',
    port: 5000,
    startTime: null,
    totalTransfer: 0,
    deviceCount: 0,
    /* Files */
    sharedFiles: [],
    sharedDir: '',
    countBadge: '0 files',
    globalProgress: { show: false, pct: 0, info: '' },
    /* QR Data */
    qrUrlData: '',
    /* Clock */
    clock: '--:--:--',
    uptimeStr: '—',
    /* Devices page */
    devices: [],
    /* Settings */
    settings: {
      autoStart: false,
      defaultPort: 5000,
      pollInterval: 4,
      notifications: true,
      sound: true,
    },

    get serverUrl() {
      if (this.running && this.serverIp && this.serverPort) {
        return 'http://' + this.serverIp + ':' + this.serverPort;
      }
      return '';
    },

    get pageTitle() {
      return { dashboard: 'Dashboard', devices: 'Connected Devices' }[this.page] || '';
    },

    get pageBreadcrumb() {
      return { dashboard: 'Host', devices: 'Network' }[this.page] || '';
    },

    init() {
      var self = this;
      this.tickClock();
      setInterval(function () { self.tickClock(); }, 1000);
      setInterval(function () { self.tickUptime(); }, 1000);
      this.bindDesktopDrop();
      /* Re-init lucide icons after Alpine renders */
      this.$nextTick(function () { self.refreshIcons(); });
      /* Watch page changes to re-init icons */
      this.$watch('page', function () {
        self.$nextTick(function () { self.refreshIcons(); });
      });
      // Fetch initial state
      setTimeout(async function () {
        try {
          self.sharedDir = await invoke('get_shared_dir');

          // Check if server is already running
          var status = await invoke('get_status');
          if (status.running) {
            self.running = true;
            self.ssid = status.ssid || '';
            self.password = status.password || '';
            self.ip = status.server_ip || '';
            self.port = status.server_port || 5000;
            self.deviceCount = status.connected_devices || 0;
            self.devices = status.devices_list || [];
            self.hotspotActive = status.hotspot_active || false;
            self.sharedFiles = status.shared_files || [];
            self.totalTransfer = status.total_transfer || 0;
            self.generateQR();
            self.startPolling();
          }
        } catch (e) { console.error('Failed to init app state:', e); }
      }, 500);

      // Listen for WebSocket presence events
      listen('device-changed', async () => {
        try {
          var status = await invoke('get_status');
          self.deviceCount = status.connected_devices || 0;
          self.devices = status.devices_list || [];
          self.sharedFiles = status.shared_files || [];
        } catch (e) { console.error('Error refreshing devices', e); }
      });

      // Desktop drag and drop events
      const dropOverlay = document.getElementById('desktop-drop-overlay');
      
      listen('tauri://drag-enter', () => {
        if (!self.running) return;
        if (dropOverlay) dropOverlay.classList.add('active');
      });
      
      listen('tauri://drag-leave', () => {
        if (dropOverlay) dropOverlay.classList.remove('active');
      });
      
      listen('tauri://drag-drop', async (event) => {
        if (dropOverlay) dropOverlay.classList.remove('active');
        if (!self.running) {
            self.toast('Start the server to share files!');
            return;
        }
        
        const paths = event.payload.paths;
        if (paths && paths.length > 0) {
            for (let p of paths) {
                try {
                    await invoke('add_shared_file', { path: p });
                } catch (e) {
                    console.error('Failed to add file:', e);
                }
            }
            var status = await invoke('get_status');
            self.sharedFiles = status.shared_files || [];
            self.toast(paths.length + ' file(s) shared!');
        }
      });
    },

    refreshIcons() {
      lucide.createIcons({
        icons: lucide.icons
      });
    },

    tickClock() {
      var now = new Date();
      this.clock = now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    },

    tickUptime() {
      if (!this.running || !this.startTime) { this.uptimeStr = '—'; return; }
      var elapsed = Math.floor((Date.now() - this.startTime) / 1000);
      var h = Math.floor(elapsed / 3600);
      var m = Math.floor((elapsed % 3600) / 60);
      var s = elapsed % 60;
      this.uptimeStr = (h > 0 ? h + 'h ' : '') + (m > 0 ? m + 'm ' : '') + s + 's';
    },

    toggleServer() {
      if (!this.running) this.startServer();
      else this.stopServer();
    },

    async startServer() {
      var self = this;
      self.loading = true;
      var ssid = self.ssid.trim() || 'Wifly';
      var pass = self.password.trim() || 'wifly1234';
      var port = parseInt(self.port) || 5000;

      try {
        var result = await invoke('start_server', {
          ssid: ssid,
          password: pass,
          port: port
        });

        self.loading = false;
        self.applyRunning(true, result.ip, result.port);
        self.$nextTick(function () { self.refreshIcons(); });
      } catch (err) {
        self.loading = false;
        self.showToast('Error: ' + err, 'err');
        self.$nextTick(function () { self.refreshIcons(); });
      }
    },

    async stopServer() {
      try {
        await invoke('stop_server');
      } catch (err) {
        console.error('Stop error:', err);
      }
      this.applyRunning(false, null, null);
      this.stopPolling();
    },

    applyRunning(state, ip, port) {
      this.running = state;
      this.serverIp = state ? ip : null;
      this.serverPort = state ? port : null;
      if (state) { this.startTime = Date.now(); }
      else { this.startTime = null; this.totalTransfer = 0; this.sharedFiles = []; this.devices = []; this.deviceCount = 0; }

      if (state) {
        this.startPolling();
        this.renderQR('http://' + ip + ':' + port);
        this.showToast('Hotspot started · ' + ip + ':' + port, 'ok');
      } else {
        this.showToast('Hotspot stopped', '');
        this.clearQR();
      }
      var self = this;
      this.$nextTick(function () { self.refreshIcons(); });
    },

    /* QR Code */
    async renderQR(url) {
      try {
        this.qrUrlData = await QRCode.toDataURL(url, {
          width: 164,
          margin: 2,
          color: {
            dark: '#080a0e',
            light: '#ffffff'
          }
        });
      } catch (error) {
        console.error(error);
      }
    },

    clearQR() {
      this.qrUrlData = '';
    },

    /* Polling — fetch status from Rust backend */
    _pollInterval: null,
    startPolling() {
      var self = this;
      self.stopPolling();
      var ms = (self.settings.pollInterval || 4) * 1000;
      self._pollInterval = setInterval(async function () {
        try {
          var status = await invoke('get_status');
          if (!status.running) { self.stopServer(); return; }
          self.deviceCount = status.connected_devices || 0;
          self.totalTransfer = status.total_transfer || 0;
          self.devices = status.devices_list || [];
          if (status.shared_files_count !== self.sharedFiles.length) {
            self.sharedFiles = await invoke('list_shared_files');
          }
        } catch (err) {
          console.error('Poll error:', err);
        }
      }, ms);
    },

    stopPolling() {
      if (this._pollInterval) { clearInterval(this._pollInterval); this._pollInterval = null; }
    },

    /* File handling — use native Tauri dialog to pick files */
    async handleFiles() {
      if (!this.running) { this.showToast('Start the server first', 'err'); return; }
      var self = this;

      try {
        // Open native file picker
        var selected = await open({
          multiple: true,
          title: 'Select files to share',
        });

        if (!selected) return; // User cancelled

        // Normalize to array
        var paths = Array.isArray(selected) ? selected : [selected];
        if (!paths.length) return;

        var added = 0;
        for (var i = 0; i < paths.length; i++) {
          try {
            var result = await invoke('add_shared_file', { path: paths[i] });
            self.sharedFiles.push({
              id: result.id,
              name: result.name,
              size: result.size,
              path: result.path,
            });
            self.totalTransfer += (result.size || 0);
            added++;
          } catch (err) {
            if (err !== 'File already shared') {
              console.error('Error adding file:', err);
            }
          }
        }

        if (added > 0) {
          self.showToast(added + ' file' + (added > 1 ? 's' : '') + ' shared', 'ok');
        } else {
          self.showToast('Files already shared', '');
        }

        self.$nextTick(function () { self.refreshIcons(); });
      } catch (err) {
        self.showToast('Error selecting files: ' + err, 'err');
      }
    },

    /* Change Upload Directory */
    async changeDirectory() {
      try {
        var selected = await open({
          directory: true,
          multiple: false,
          title: 'Select Folder for Uploads',
          defaultPath: this.sharedDir
        });

        if (selected) {
          await invoke('set_shared_dir', { path: selected });
          this.sharedDir = selected;
          this.showToast('Upload folder changed', 'ok');
        }
      } catch (err) {
        console.error('Failed to change directory:', err);
        this.showToast('Failed to change folder', 'err');
      }
    },

    /* Handle drag-and-drop files */
    async handleDroppedFiles(files) {
      if (!this.running) { this.showToast('Start the server first', 'err'); return; }
      var self = this;

      // For drag-and-drop in Tauri webview, we get File objects
      // We need to use the file path if available, otherwise use temp storage
      var added = 0;
      for (var i = 0; i < files.length; i++) {
        var file = files[i];
        // In Tauri, dropped files may have a path property
        var filePath = file.path || file.name;
        try {
          var result = await invoke('add_shared_file', { path: filePath });
          self.sharedFiles.push({
            id: result.id,
            name: result.name,
            size: result.size,
            path: result.path,
          });
          self.totalTransfer += (result.size || 0);
          added++;
        } catch (err) {
          if (err !== 'File already shared') {
            console.error('Error adding dropped file:', err);
          }
        }
      }

      if (added > 0) {
        self.showToast(added + ' file' + (added > 1 ? 's' : '') + ' shared', 'ok');
      }
      self.$nextTick(function () { self.refreshIcons(); });
    },

    /* Remove a shared file */
    async removeSharedFile(index) {
      var file = this.sharedFiles[index];
      if (!file) return;
      try {
        await invoke('remove_shared_file', { fileId: file.id });
      } catch (err) {
        console.error('Error removing file:', err);
      }
      this.sharedFiles.splice(index, 1);
    },

    /* Desktop drag-and-drop */
    bindDesktopDrop() {
      var overlay = document.getElementById('desktop-drop-overlay');
      var dragCounter = 0;
      var self = this;
      document.addEventListener('dragenter', function (e) { e.preventDefault(); dragCounter++; overlay.classList.add('active'); });
      document.addEventListener('dragleave', function (e) { e.preventDefault(); dragCounter--; if (dragCounter <= 0) { dragCounter = 0; overlay.classList.remove('active'); } });
      document.addEventListener('dragover', function (e) { e.preventDefault(); });
      document.addEventListener('drop', function (e) {
        e.preventDefault(); dragCounter = 0; overlay.classList.remove('active');
        if (e.dataTransfer.files.length) self.handleDroppedFiles(Array.from(e.dataTransfer.files));
      });

      /* Drop zone specific */
      var dz = document.getElementById('drop-zone');
      if (dz) {
        dz.addEventListener('dragover', function (e) { e.preventDefault(); dz.classList.add('drag-over'); });
        dz.addEventListener('dragleave', function () { dz.classList.remove('drag-over'); });
        dz.addEventListener('drop', function (e) {
          e.preventDefault(); dz.classList.remove('drag-over');
          self.handleDroppedFiles(Array.from(e.dataTransfer.files));
        });
      }
    },

    /* Settings */
    clearHistory() {
      this.sharedFiles = [];
      this.totalTransfer = 0;
      this.devices = [];
      this.showToast('History cleared', 'ok');
    },

    /* Utilities */
    togglePass() {
      var inp = document.getElementById('pass-input');
      if (!inp) return;
      inp.type = (inp.type === 'password') ? 'text' : 'password';
    },

    copyEl(elId, msg) {
      var el = document.getElementById(elId);
      if (!el) return;
      var val = el.textContent;
      if (!val || val === '—') return;
      navigator.clipboard.writeText(val).then(function () { this.showToast(msg, 'ok'); }.bind(this));
    },

    copyPill() {
      if (!this.running) return;
      var self = this;
      navigator.clipboard.writeText(self.serverUrl).then(function () { self.showToast('URL copied!', 'ok'); });
    },

    _toastT: null,
    showToast(msg, type) {
      var t = document.getElementById('toast');
      t.textContent = msg;
      t.className = 'toast ' + (type || '') + ' show';
      clearTimeout(this._toastT);
      this._toastT = setTimeout(function () { t.classList.remove('show'); }, 2800);
    },

    fmtSize(b) {
      if (!b) return '—';
      if (b < 1024) return b + ' o';
      if (b < 1048576) return (b / 1024).toFixed(1) + ' Ko';
      if (b < 1073741824) return (b / 1048576).toFixed(1) + ' Mo';
      return (b / 1073741824).toFixed(2) + ' Go';
    },

    getEmoji(name) {
      var ext = (name.split('.').pop() || '').toLowerCase();
      var m = {
        pdf: '📄', doc: '📝', docx: '📝', xls: '📊', xlsx: '📊', ppt: '📑', pptx: '📑',
        jpg: '🖼', jpeg: '🖼', png: '🖼', gif: '🖼', webp: '🖼', svg: '🖼', ico: '🖼',
        mp4: '🎬', mov: '🎬', avi: '🎬', mkv: '🎬', webm: '🎬',
        mp3: '🎵', wav: '🎵', flac: '🎵', ogg: '🎵',
        zip: '🗜', rar: '🗜', '7z': '🗜', tar: '🗜', gz: '🗜',
        js: '💻', ts: '💻', py: '💻', html: '💻', css: '💻', json: '💻',
        txt: '📄', md: '📄', csv: '📊', sql: '🗃'
      };
      return m[ext] || '📦';
    },

    esc(s) {
      return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    },

    /* Window Controls */
    minimizeWindow() {
      appWindow.minimize();
    },
    toggleMaximize() {
      appWindow.toggleMaximize();
    },

    closeWindow() {
      // Stop server before closing
      if (this.running) {
        invoke('stop_server').catch(function () { }).finally(function () {
          appWindow.close();
        });
      } else {
        appWindow.close();
      }
    }
  };
}

window.wiflyApp = wiflyApp;
Alpine.start();
