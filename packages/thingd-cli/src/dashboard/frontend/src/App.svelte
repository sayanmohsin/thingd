<script>
  import { onMount, onDestroy } from 'svelte';
  import { flip } from 'svelte/animate';
  import { fly, fade } from 'svelte/transition';
  import JsonEditor from './lib/JsonEditor.svelte';

  // App State
  let currentTab = 'metrics';
  let dbStatus = 'Connecting...';
  let dbDriver = '-';
  let dbPath = '-';
  let dbSize = 'N/A';
  let metrics = { objects: 0, events: 0, activeJobs: 0, deadJobs: 0 };
  let dbHealth = { integrityOk: false, integrityMessage: 'Not checked', walMode: '-', walFrames: 0, backupPath: '', backupSize: 0 };
  let serverVersion = '';
  let diagnosticLogs = [
    { time: new Date().toLocaleTimeString(), text: 'Svelte Dashboard loaded successfully.', type: 'muted' },
    { time: new Date().toLocaleTimeString(), text: 'Establishing database connections...', type: 'info' }
  ];

  // Auth Key Gate
  let authToken = localStorage.getItem('thingd_auth_token') || '';
  let authGateActive = false;
  let inputAuthToken = '';

  // Lists & Selections
  let collections = [];
  let selectedCollection = null;
  let objects = [];
  let streams = [];
  let selectedStream = null;
  let events = [];
  let queues = [];
  let selectedQueue = null;
  let queueStats = { queue: '', totalActive: 0, ready: 0, leased: 0, dead: 0 };
  let activeJobs = [];
  let deadJobs = [];
  let currentQueueSubtab = 'active';
  let selectedJob = null;
  
  // Search Sandbox
  let searchQuery = '';
  let searchLimit = 20;
  let searchCollections = '';
  let searchMetadata = '';
  let searchResults = [];

  // NLQ
  let nlqQuestion = '';
  let nlqCollection = '';
  let nlqResults = null;
  let nlqLoading = false;
  let nlqSchema = null;
  let nlqShowSchema = false;
  let nlqModel = localStorage.getItem('thingd_nlq_model') || 'llama3';
  let nlqEndpoint = localStorage.getItem('thingd_nlq_endpoint') || 'http://localhost:11434/v1';
  let nlqApiKey = localStorage.getItem('thingd_nlq_api_key') || '';
  let nlqShowSettings = false;

  // Recently changed items (for highlight animation)
  let recentlyChanged = new Set();
  function markChanged(id) {
    recentlyChanged.add(id);
    recentlyChanged = recentlyChanged;
    setTimeout(() => {
      recentlyChanged.delete(id);
      recentlyChanged = recentlyChanged;
    }, 1500);
  }

  // Modals Visibility
  let modalObjectVisible = false;
  let modalEventVisible = false;
  let modalPushJobVisible = false;
  let modalClaimJobVisible = false;
  let modalJobOpsVisible = false;
  let modalConnVisible = false;
  let editObjectMode = false;
  let nackFormVisible = false;

  // Form Fields
  let objId = '';
  let objText = '';
  let objData = '';
  
  let eventStream = '';
  let eventType = '';
  let eventText = '';
  let eventData = '';

  let jobPayload = '';
  let jobDelay = 0;
  let jobAttempts = 5;
  let jobIdempotency = '';

  let claimLease = 60000;

  let nackError = '';
  let nackDelay = 0;

  // Connection settings fields
  let connDriver = 'memory';
  let connPath = ':memory:';
  let connToken = '';

  // Toast System
  let toasts = [];
  function showToast(message, type = 'info') {
    const id = Date.now() + Math.random();
    toasts = [...toasts, { id, message, type }];
    setTimeout(() => {
      toasts = toasts.filter(t => t.id !== id);
    }, 4000);
  }

  function logDiagnostic(text, type = '') {
    const time = new Date().toLocaleTimeString();
    diagnosticLogs = [...diagnosticLogs, { time, text, type }];
    if (diagnosticLogs.length > 50) {
      diagnosticLogs = diagnosticLogs.slice(-50);
    }
  }

  // Escape HTML helper
  function escapeHtml(str) {
    if (!str) return '';
    return str.toString()
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }

  // REST API requester
  async function request(url, options = {}) {
    const headers = { 'Content-Type': 'application/json' };
    if (authToken) {
      headers['Authorization'] = `Bearer ${authToken}`;
    }
    
    try {
      const response = await fetch(url, { headers, ...options });
      
      // Auto-detect authentication required states
      if (response.status === 401) {
        authGateActive = true;
        showToast('Authentication token required.', 'warning');
        throw new Error('Unauthorized');
      }
      
      const isJson = response.headers.get('content-type')?.includes('application/json');
      const data = isJson ? await response.json() : await response.text();
      
      if (!response.ok) {
        throw new Error(data?.error || `HTTP ${response.status}: ${response.statusText}`);
      }
      return data;
    } catch (err) {
      console.error(`API Error on ${url}:`, err);
      throw err;
    }
  }

  // Status Poller
  async function fetchStatus() {
    try {
      const res = await request('/api/status');
      dbStatus = 'Active (Localhost)';
      dbDriver = res.driver === 'memory' ? 'In-Memory Store' : res.driver === 'native' ? 'Native SQLite (FTS5)' : 'Cloud Endpoint';
      dbPath = res.path;
      dbSize = res.metrics.dbSize || 'N/A';
      metrics = res.metrics;
      serverVersion = res.version || '';
      authGateActive = false;
    } catch (err) {
      if (err.message !== 'Unauthorized') {
        dbStatus = 'Disconnected';
        dbDriver = 'Disconnected';
        dbPath = '-';
        dbSize = 'N/A';
        logDiagnostic(`Status synchronization failed: ${err.message}`, 'danger');
      }
    }
  }

  // Login handler
  function handleLogin(e) {
    e.preventDefault();
    if (!inputAuthToken.trim()) {
      showToast('Token is required.', 'warning');
      return;
    }
    authToken = inputAuthToken.trim();
    localStorage.setItem('thingd_auth_token', authToken);
    authGateActive = false;
    showToast('Token registered successfully.', 'success');
    logDiagnostic('Custom security authorization key set.');
    syncAll();
  }

  // Log out handler
  function handleLogout() {
    authToken = '';
    localStorage.removeItem('thingd_auth_token');
    authGateActive = true;
    showToast('Secure session logged out.', 'info');
  }

  // General Sync
  function syncAll() {
    fetchStatus();
    if (currentTab === 'collections') fetchCollections();
    if (currentTab === 'events') fetchStreams();
    if (currentTab === 'queues') fetchQueues();
  }

  // Tab router
  function selectTab(tab) {
    currentTab = tab;
    if (tab === 'collections') {
      fetchCollections();
    } else if (tab === 'events') {
      fetchStreams();
    } else if (tab === 'queues') {
      fetchQueues();
    }
  }

  // Collections & Objects
  async function fetchCollections() {
    try {
      collections = await request('/api/collections');
    } catch (err) {
      if (err.message !== 'Unauthorized') {
        showToast(`Failed to fetch collections: ${err.message}`, 'error');
      }
    }
  }

  async function selectCollection(col) {
    selectedCollection = col;
    fetchObjects();
  }

  async function fetchObjects() {
    if (!selectedCollection) return;
    try {
      objects = await request(`/api/objects?collection=${encodeURIComponent(selectedCollection)}`);
    } catch (err) {
      if (err.message !== 'Unauthorized') {
        showToast(`Failed to load objects: ${err.message}`, 'error');
      }
    }
  }

  function openCreateObjectModal() {
    editObjectMode = false;
    objId = '';
    objText = '';
    objData = '';
    modalObjectVisible = true;
  }

  function openEditObjectModal(obj) {
    editObjectMode = true;
    objId = obj.id;
    objText = obj.text || '';
    
    const meta = { ...obj };
    delete meta.id;
    delete meta.text;
    delete meta.collection;
    delete meta.createdAt;
    delete meta.updatedAt;
    delete meta.version;
    objData = Object.keys(meta).length > 0 ? JSON.stringify(meta, null, 2) : '';
    
    modalObjectVisible = true;
  }

  async function saveObject(e) {
    e.preventDefault();
    let data = {};
    if (objData.trim()) {
      try {
        data = JSON.parse(objData);
      } catch (err) {
        showToast('Custom data must be valid JSON.', 'error');
        return;
      }
    }
    try {
      await request('/api/objects', {
        method: 'POST',
        body: JSON.stringify({
          collection: selectedCollection,
          id: objId,
          text: objText,
          data
        })
      });
      showToast(`Object "${objId}" saved successfully.`, 'success');
      logDiagnostic(`Saved object "${objId}" in collection "${selectedCollection}".`);
      modalObjectVisible = false;
      markChanged(objId);
      fetchObjects();
      fetchStatus();
    } catch (err) {
      showToast(`Save failed: ${err.message}`, 'error');
    }
  }

  async function deleteObject(id) {
    if (!confirm(`Are you sure you want to delete object "${id}"?`)) return;
    try {
      await request(`/api/objects?collection=${encodeURIComponent(selectedCollection)}&id=${encodeURIComponent(id)}`, {
        method: 'DELETE'
      });
      showToast(`Object "${id}" deleted.`, 'success');
      logDiagnostic(`Deleted object "${id}" from collection "${selectedCollection}".`, 'warning');
      fetchObjects();
      fetchStatus();
    } catch (err) {
      showToast(`Deletion failed: ${err.message}`, 'error');
    }
  }

  // Events & Streams
  async function fetchStreams() {
    try {
      streams = await request('/api/events/streams');
    } catch (err) {
      if (err.message !== 'Unauthorized') {
        showToast(`Failed to fetch event streams: ${err.message}`, 'error');
      }
    }
  }

  function selectStream(stream) {
    selectedStream = stream;
    fetchEvents();
  }

  async function fetchEvents() {
    if (!selectedStream) return;
    try {
      events = await request(`/api/events?stream=${encodeURIComponent(selectedStream)}`);
    } catch (err) {
      if (err.message !== 'Unauthorized') {
        showToast(`Failed to load event timeline: ${err.message}`, 'error');
      }
    }
  }

  function openAppendEventModal() {
    eventStream = selectedStream || '';
    eventType = '';
    eventText = '';
    eventData = '';
    modalEventVisible = true;
  }

  async function appendEvent(e) {
    e.preventDefault();
    let data = {};
    if (eventData.trim()) {
      try {
        data = JSON.parse(eventData);
      } catch (err) {
        showToast('Event payload must be valid JSON.', 'error');
        return;
      }
    }
    try {
      await request('/api/events', {
        method: 'POST',
        body: JSON.stringify({
          stream: eventStream,
          type: eventType,
          text: eventText,
          data
        })
      });
      showToast('Event logged successfully.', 'success');
      logDiagnostic(`Appended event [${eventType}] to stream "${eventStream}".`);
      modalEventVisible = false;
      markChanged(eventStream);
      fetchStreams();
      if (selectedStream === eventStream) {
        fetchEvents();
      } else {
        selectStream(eventStream);
      }
      fetchStatus();
    } catch (err) {
      showToast(`Failed to append event: ${err.message}`, 'error');
    }
  }

  // Queues & Jobs
  async function fetchQueues() {
    try {
      queues = await request('/api/queues');
    } catch (err) {
      if (err.message !== 'Unauthorized') {
        showToast(`Failed to fetch queues: ${err.message}`, 'error');
      }
    }
  }

  async function selectQueue(q) {
    selectedQueue = q;
    fetchQueueStats();
    fetchQueueJobs();
  }

  async function fetchQueueStats() {
    if (!selectedQueue) return;
    try {
      queueStats = await request(`/api/queues/stats?queue=${encodeURIComponent(selectedQueue)}`);
    } catch (err) {
      console.error(err);
    }
  }

  async function fetchQueueJobs() {
    if (!selectedQueue) return;
    try {
      const [active, dead] = await Promise.all([
        request(`/api/queues/jobs?queue=${encodeURIComponent(selectedQueue)}&status=active`),
        request(`/api/queues/jobs?queue=${encodeURIComponent(selectedQueue)}&status=dead`)
      ]);
      activeJobs = active;
      deadJobs = dead;
    } catch (err) {
      if (err.message !== 'Unauthorized') {
        showToast(`Failed to load jobs list: ${err.message}`, 'error');
      }
    }
  }

  function openPushJobModal() {
    jobPayload = '';
    jobDelay = 0;
    jobAttempts = 5;
    jobIdempotency = '';
    modalPushJobVisible = true;
  }

  async function pushJob(e) {
    e.preventDefault();
    let payload = {};
    try {
      payload = JSON.parse(jobPayload);
    } catch (err) {
      showToast('Payload must be valid JSON object.', 'error');
      return;
    }
    try {
      const result = await request('/api/queues/push', {
        method: 'POST',
        body: JSON.stringify({
          queue: selectedQueue,
          payload,
          delayMs: jobDelay,
          maxAttempts: jobAttempts,
          idempotencyKey: jobIdempotency || undefined
        })
      });
      showToast(`Job "${result.id}" pushed onto queue.`, 'success');
      logDiagnostic(`Pushed job ID "${result.id}" onto queue "${selectedQueue}".`);
      modalPushJobVisible = false;
      markChanged(result.id);
      fetchQueueStats();
      fetchQueueJobs();
      fetchStatus();
    } catch (err) {
      showToast(`Failed to push job: ${err.message}`, 'error');
    }
  }

  function openClaimJobModal() {
    claimLease = 60000;
    modalClaimJobVisible = true;
  }

  async function claimJob(e) {
    e.preventDefault();
    try {
      const job = await request('/api/queues/claim', {
        method: 'POST',
        body: JSON.stringify({
          queue: selectedQueue,
          leaseMs: claimLease
        })
      });
      
      if (!job) {
        showToast('No ready jobs available in queue.', 'warning');
        logDiagnostic(`No ready jobs available in queue "${selectedQueue}".`, 'warning');
        modalClaimJobVisible = false;
        return;
      }
      
      showToast(`Claimed job ID: "${job.id}"`, 'success');
      logDiagnostic(`Claimed job ID "${job.id}" from queue "${selectedQueue}".`, 'success');
      modalClaimJobVisible = false;
      
      fetchQueueStats();
      fetchQueueJobs();
      fetchStatus();
      
      // Inspect claimed job
      openJobOpsModal(job);
    } catch (err) {
      showToast(`Claim failed: ${err.message}`, 'error');
    }
  }

  function openJobOpsModal(job) {
    selectedJob = job;
    nackFormVisible = false;
    nackError = '';
    nackDelay = 0;
    modalJobOpsVisible = true;
  }

  async function ackJob() {
    if (!selectedJob) return;
    const jobId = selectedJob.id;
    try {
      await request('/api/queues/ack', {
        method: 'POST',
        body: JSON.stringify({
          queue: selectedQueue,
          jobId
        })
      });
      showToast(`Job "${jobId}" acknowledged (ACK).`, 'success');
      logDiagnostic(`Acknowledged (ACK) job "${jobId}" from queue "${selectedQueue}".`, 'success');
      modalJobOpsVisible = false;
      fetchQueueStats();
      fetchQueueJobs();
      fetchStatus();
    } catch (err) {
      showToast(`ACK failed: ${err.message}`, 'error');
    }
  }

  async function nackJob(e) {
    e.preventDefault();
    if (!selectedJob) return;
    const jobId = selectedJob.id;
    try {
      await request('/api/queues/nack', {
        method: 'POST',
        body: JSON.stringify({
          queue: selectedQueue,
          jobId,
          error: nackError || undefined,
          delayMs: nackDelay
        })
      });
      showToast(`Job "${jobId}" rejected (NACK).`, 'warning');
      logDiagnostic(`Rejected (NACK) job "${jobId}" from queue "${selectedQueue}".`, 'warning');
      modalJobOpsVisible = false;
      fetchQueueStats();
      fetchQueueJobs();
      fetchStatus();
    } catch (err) {
      showToast(`NACK failed: ${err.message}`, 'error');
    }
  }

  // FTS5 Search Sandbox
  async function runSearch() {
    const query = searchQuery.trim();
    if (!query) {
      showToast('Search query is required.', 'warning');
      return;
    }
    
    searchResults = [];
    let url = `/api/search?query=${encodeURIComponent(query)}&limit=${searchLimit}`;
    if (searchCollections.trim()) {
      url += `&collections=${encodeURIComponent(searchCollections)}`;
    }
    if (searchMetadata.trim()) {
      try {
        JSON.parse(searchMetadata); // validate JSON
        url += `&filter=${encodeURIComponent(searchMetadata)}`;
      } catch {
        showToast('Metadata filter must be a valid JSON.', 'error');
        return;
      }
    }
    
    try {
      searchResults = await request(url);
    } catch (err) {
      if (err.message !== 'Unauthorized') {
        showToast(`Search failed: ${err.message}`, 'error');
      }
    }
  }

  // NLQ Query
  async function runNlq() {
    const question = nlqQuestion.trim();
    if (!question) {
      showToast('Please enter a question.', 'warning');
      return;
    }

    nlqLoading = true;
    nlqResults = null;

    saveNlqSettings();

    try {
      const response = await fetch('/api/nlq', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          question,
          collection: nlqCollection || undefined,
          model: nlqModel,
          endpoint: nlqEndpoint,
          apiKey: nlqApiKey,
        }),
      });
      const data = await response.json();
      if (!response.ok) {
        showToast(data.error || 'NLQ request failed', 'error');
        return;
      }
      nlqResults = data;
    } catch (err) {
      showToast(`NLQ failed: ${err.message}`, 'error');
    } finally {
      nlqLoading = false;
    }
  }

  async function fetchNlqSchema() {
    try {
      const url = nlqCollection ? `/api/schema?collection=${encodeURIComponent(nlqCollection)}` : '/api/schema';
      nlqSchema = await request(url);
    } catch (err) {
      showToast(`Schema fetch failed: ${err.message}`, 'error');
    }
  }

  function saveNlqSettings() {
    localStorage.setItem('thingd_nlq_model', nlqModel);
    localStorage.setItem('thingd_nlq_endpoint', nlqEndpoint);
    localStorage.setItem('thingd_nlq_api_key', nlqApiKey);
  }

  function formatNlqValue(val) {
    if (typeof val === 'number') return val.toLocaleString();
    return String(val ?? '');
  }

  function getHighlightedText(text, query) {
    if (!text) return '-';
    let res = escapeHtml(text);
    try {
      const words = query.replace(/[^\w\s]/g, '').split(/\s+/).filter(w => w.length > 2);
      words.forEach(word => {
        const regex = new RegExp(`(${word})`, 'gi');
        res = res.replace(regex, '<span class="highlighted-match">$1</span>');
      });
    } catch {
      // Ignore highlighter exception
    }
    return res;
  }

  async function runIntegrityCheck() {
    try {
      const result = await request('/api/db/integrity');
      dbHealth = { ...dbHealth, integrityOk: result.ok, integrityMessage: result.message };
      showToast(result.ok ? 'Integrity check passed' : `Integrity check failed: ${result.message}`, result.ok ? 'success' : 'error');
    } catch (err) {
      showToast(`Integrity check failed: ${err.message}`, 'error');
    }
  }

  async function runWalCheckpoint() {
    try {
      const result = await request('/api/db/checkpoint');
      dbHealth = { ...dbHealth, walFrames: result.framesAfter || 0 };
      showToast(`WAL checkpoint complete (${result.framesBefore || 0} frames before)`, 'success');
    } catch (err) {
      showToast(`WAL checkpoint failed: ${err.message}`, 'error');
    }
  }

  async function createBackup() {
    const backupPath = prompt('Enter backup file path:', `thingd-backup-${Date.now()}.db`);
    if (!backupPath) return;
    try {
      const result = await request('/api/backup', {
        method: 'POST',
        body: JSON.stringify({ path: backupPath })
      });
      dbHealth = { ...dbHealth, backupPath: result.path, backupSize: result.sizeBytes };
      showToast(`Backup created: ${result.path}`, 'success');
    } catch (err) {
      showToast(`Backup failed: ${err.message}`, 'error');
    }
  }

  // Connection settings
  function openConnSettingsModal() {
    connDriver = dbDriver.includes('In-Memory') ? 'memory' : dbDriver.includes('FTS5') ? 'native' : 'cloud';
    connPath = dbPath;
    connToken = authToken;
    modalConnVisible = true;
  }

  async function handleConnSwitch(e) {
    e.preventDefault();
    try {
      await request('/api/connect', {
        method: 'POST',
        body: JSON.stringify({
          path: connPath,
          driver: connDriver,
          authToken: connToken || undefined
        })
      });
      
      // Update local storage credentials
      authToken = connToken;
      if (authToken) {
        localStorage.setItem('thingd_auth_token', authToken);
      } else {
        localStorage.removeItem('thingd_auth_token');
      }
      
      showToast('Database connection swapped successfully.', 'success');
      logDiagnostic(`Swapped database connection to: [${connDriver}] ${connPath}`);
      modalConnVisible = false;
      
      // Reset active tabs
      selectedCollection = null;
      selectedStream = null;
      selectedQueue = null;
      
      syncAll();
    } catch (err) {
      showToast(`Connection failed: ${err.message}`, 'error');
    }
  }

  // Background Pollers
  let pollingInterval = null;
  let viewInterval = null;

  onMount(() => {
    fetchStatus();
    logDiagnostic('DB Connection resolved: Active local SQLite stemming context.', 'success');
    pollingInterval = setInterval(fetchStatus, 4000);
    
    // Auto-polling for active lists
    viewInterval = setInterval(() => {
      if (currentTab === 'events' && selectedStream) fetchEvents();
      if (currentTab === 'queues' && selectedQueue) {
        fetchQueueStats();
        fetchQueueJobs();
      }
    }, 3000);
  });

  onDestroy(() => {
    if (pollingInterval) clearInterval(pollingInterval);
    if (viewInterval) clearInterval(viewInterval);
  });
</script>

<!-- Security Portal (Authorized Gate) -->
{#if authGateActive}
  <div class="modal-overlay active" style="z-index: 10000; background: rgba(5, 6, 12, 0.95);">
    <div class="modal-card" style="border-color: rgba(99, 102, 241, 0.3); box-shadow: 0 0 40px rgba(99, 102, 241, 0.25);">
      <div class="modal-header" style="justify-content: center; border-bottom: none; padding-top: 30px;">
        <span class="brand-logo" style="width: 44px; height: 44px; font-size: 18px;">tg</span>
      </div>
      <div class="modal-body" style="padding-top: 10px;">
        <h2 class="text-center" style="font-size: 20px; font-weight: 700; margin-bottom: 8px;">thingd Security Shield</h2>
        <p class="text-muted text-center" style="font-size: 13.5px; margin-bottom: 24px;">This database instance is protected. Enter the authorization bearer token to gain access.</p>
        
        <form on:submit={handleLogin}>
          <div class="form-group">
            <label for="auth-key">Authorization Bearer Token *</label>
            <input type="password" id="auth-key" bind:value={inputAuthToken} placeholder="Enter auth token..." class="form-input text-center" style="letter-spacing: 2px;" required>
          </div>
          <div class="modal-footer" style="justify-content: center; margin-top: 20px;">
            <button type="submit" class="btn btn-primary" style="width: 100%; height: 42px;">Unlock Dashboard</button>
          </div>
        </form>
      </div>
    </div>
  </div>
{/if}

<div class="app-container">
  <!-- Sidebar -->
  <aside class="sidebar">
    <div class="sidebar-brand">
      <span class="brand-logo">tg</span>
      <span class="brand-text">thingd<span class="version-badge">{serverVersion || 'v0.3.0'}</span></span>
    </div>
    
    <nav class="sidebar-nav">
      <button class="nav-item {currentTab === 'metrics' ? 'active' : ''}" on:click={() => selectTab('metrics')}>
        <svg class="nav-icon" viewBox="0 0 24 24"><path d="M19 3H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM9 17H7v-7h2v7zm4 0h-2V7h2v10zm4 0h-2v-4h2v4z"/></svg>
        Dashboard Status
      </button>
      <button class="nav-item {currentTab === 'collections' ? 'active' : ''}" on:click={() => selectTab('collections')}>
        <svg class="nav-icon" viewBox="0 0 24 24"><path d="M4 6H2v14c0 1.1.9 2 2 2h14v-2H4V6zm16-4H8c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h12c1.1 0-2-.9-2-2V4c0-1.1-.9-2-2-2zm0 14H8V4h12v12z"/></svg>
        Collections & Objects
      </button>
      <button class="nav-item {currentTab === 'events' ? 'active' : ''}" on:click={() => selectTab('events')}>
        <svg class="nav-icon" viewBox="0 0 24 24"><path d="M13 3c-4.97 0-9 4.03-9 9H1l3.89 3.89.07.14L9 12H6c0-3.87 3.13-7 7-7s7 3.13 7 7-3.13 7-7 7c-1.93 0-3.68-.79-4.94-2.06l-1.42 1.42C8.27 19.99 10.51 21 13 21c4.97 0 9-4.03 9-9s-4.03-9-9-9zm-1 5v5l4.28 2.54.72-1.21-3.5-2.08V8H12z"/></svg>
        Event Log Stream
      </button>
      <button class="nav-item {currentTab === 'queues' ? 'active' : ''}" on:click={() => selectTab('queues')}>
        <svg class="nav-icon" viewBox="0 0 24 24"><path d="M19 15v3H5v-3h14m2-2H3v7h18v-7zM19 5v3H5V5h14m2-2H3v7h18V3z"/></svg>
        Queues & Background Jobs
      </button>
      <button class="nav-item {currentTab === 'search' ? 'active' : ''}" on:click={() => selectTab('search')}>
        <svg class="nav-icon" viewBox="0 0 24 24"><path d="M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z"/></svg>
        Stemming FTS5 Tester
      </button>
      <button class="nav-item {currentTab === 'nlq' ? 'active' : ''}" on:click={() => selectTab('nlq')}>
        <svg class="nav-icon" viewBox="0 0 24 24"><path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm0 14H5.17L4 17.17V4h16v12z"/></svg>
        NLQ Query
      </button>
      <button class="nav-item {currentTab === 'health' ? 'active' : ''}" on:click={() => selectTab('health')}>
        <svg class="nav-icon" viewBox="0 0 24 24"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z"/></svg>
        Database Health
      </button>
    </nav>

    <div class="sidebar-footer" style="display: flex; flex-direction: column; gap: 10px;">
      <div class="status-indicator">
        <span class="pulse-dot"></span>
        <span class="status-text">{dbStatus}</span>
      </div>
      {#if authToken}
        <button class="btn btn-sm btn-secondary" on:click={handleLogout} style="width: 100%;">Lock Session</button>
      {/if}
    </div>
  </aside>

  <!-- Main Panel -->
  <main class="main-panel">
    <!-- Topbar Header -->
    <header class="topbar">
      <div class="topbar-left">
        <h1>
          {#if currentTab === 'metrics'}Dashboard Status{/if}
          {#if currentTab === 'collections'}Collections & Objects{/if}
          {#if currentTab === 'events'}Event Log Stream{/if}
          {#if currentTab === 'queues'}Queues & Background Jobs{/if}
          {#if currentTab === 'search'}Stemming FTS5 Tester{/if}
          {#if currentTab === 'nlq'}NLQ Query{/if}
          {#if currentTab === 'health'}Database Health{/if}
        </h1>
      </div>
      <div class="topbar-right">
        <div class="connection-meta">
          <span class="meta-label">Driver:</span>
          <span class="meta-val">{dbDriver}</span>
          <span class="meta-sep">|</span>
          <span class="meta-label">Path:</span>
          <span class="meta-val path-val" title={dbPath}>{dbPath}</span>
          <span class="meta-sep">|</span>
          <span class="meta-label">Size:</span>
          <span class="meta-val">{dbSize}</span>
        </div>
        <button class="btn btn-secondary btn-icon" on:click={openConnSettingsModal}>
          <svg viewBox="0 0 24 24" width="16" height="16" style="fill: currentColor;"><path d="M19.14,12.94c0.04-0.3,0.06-0.61,0.06-0.94c0-0.32-0.02-0.64-0.07-0.94l2.03-1.58c0.18-0.14,0.23-0.41,0.12-0.61l-1.92-3.32c-0.12-0.22-0.37-0.29-0.59-0.22l-2.39,0.96c-0.5-0.38-1.03-0.7-1.62-0.94L14.4,2.81c-0.04-0.24-0.24-0.41-0.48-0.41h-3.84c-0.24,0-0.43,0.17-0.47,0.41L9.25,5.35C8.66,5.59,8.12,5.92,7.63,6.29L5.24,5.33c-0.22-0.08-0.47,0-0.59,0.22L2.74,8.87c-0.12,0.21-0.08,0.47,0.12,0.61l2.03,1.58C4.84,11.36,4.8,11.69,4.8,12c0,0.31,0.04,0.64,0.09,0.94l-2.03,1.58c-0.18,0.14-0.23,0.41-0.12,0.61l1.92,3.32c0.12,0.22,0.37,0.29,0.59,0.22l2.39-0.96c0.5,0.38,1.03,0.7,1.62,0.94l0.36,2.54c0.05,0.24,0.24,0.41,0.48,0.41h3.84c0.24,0,0.44-0.17,0.47-0.41l0.36-2.54c0.59-0.24,1.13-0.56,1.62-0.94l2.39,0.96c0.22,0.08,0.47,0,0.59-0.22l1.92-3.32c0.12-0.22,0.07-0.47-0.12-0.61L19.14,12.94z M12,15.6c-1.98,0-3.6-1.62-3.6-3.6c0-1.98,1.62-3.6,3.6-3.6s3.6,1.62,3.6,3.6C15.6,13.98,13.98,15.6,12,15.6z"/></svg>
          Settings
        </button>
        <button class="btn btn-secondary btn-icon" on:click={syncAll}>
          <svg viewBox="0 0 24 24" width="16" height="16" style="fill: currentColor;"><path d="M17.65 6.35C16.2 4.9 14.21 4 12 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08c-.82 2.33-3.04 4-5.65 4-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z"/></svg>
          Refresh
        </button>
      </div>
    </header>

    <!-- View Panels -->
    <div class="view-viewport">
      
      <!-- Metrics Dashboard -->
      {#if currentTab === 'metrics'}
        <div class="tab-panel active">
          <div class="metrics-grid">
            <div class="metric-card">
              <div class="metric-icon objects-icon">
                <svg viewBox="0 0 24 24"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 17h-2v-2h2v2zm2.07-7.75l-.9.92C13.45 12.9 13 13.5 13 15h-2v-.5c0-1.1.45-2.1 1.17-2.83l1.24-1.26c.37-.36.59-.86.59-1.41 0-1.1-.9-2-2-2s-2 .9-2 2H7c0-2.76 2.24-5 5-5s5 2.24 5 5c0 1.04-.42 1.99-1.07 2.75z"/></svg>
              </div>
              <div class="metric-body">
                <span class="metric-num">{metrics.objects}</span>
                <span class="metric-title">Total Memory Objects</span>
              </div>
            </div>
            
            <div class="metric-card">
              <div class="metric-icon events-icon">
                <svg viewBox="0 0 24 24"><path d="M13 3c-4.97 0-9 4.03-9 9H1l3.89 3.89.07.14L9 12H6c0-3.87 3.13-7 7-7s7 3.13 7 7-3.13 7-7 7c-1.93 0-3.68-.79-4.94-2.06l-1.42 1.42C8.27 19.99 10.51 21 13 21c4.97 0 9-4.03 9-9s-4.03-9-9-9zm-1 5v5l4.28 2.54.72-1.21-3.5-2.08V8H12z"/></svg>
              </div>
              <div class="metric-body">
                <span class="metric-num">{metrics.events}</span>
                <span class="metric-title">Appended Events</span>
              </div>
            </div>

            <div class="metric-card">
              <div class="metric-icon queues-icon">
                <svg viewBox="0 0 24 24"><path d="M19 15v3H5v-3h14m2-2H3v7h18v-7zM19 5v3H5V5h14m2-2H3v7h18V3z"/></svg>
              </div>
              <div class="metric-body">
                <span class="metric-num">{metrics.activeJobs}</span>
                <span class="metric-title">Active Queue Jobs</span>
              </div>
            </div>

            <div class="metric-card">
              <div class="metric-icon dead-icon">
                <svg viewBox="0 0 24 24"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z"/></svg>
              </div>
              <div class="metric-body">
                <span class="metric-num">{metrics.deadJobs}</span>
                <span class="metric-title">Dead / Poison Jobs</span>
              </div>
            </div>
          </div>

          <div class="row-flex">
            <div class="card glass flex-2">
              <div class="card-header">
                <h2>Memory Engine Overview</h2>
              </div>
              <div class="card-body">
                <div class="info-table">
                  <div class="info-row">
                    <span class="info-label">Active Connection</span>
                    <span class="info-val code" title={dbPath}>{dbPath}</span>
                  </div>
                  <div class="info-row">
                    <span class="info-label">Storage Engine</span>
                    <span class="info-val">{dbDriver}</span>
                  </div>
                  <div class="info-row">
                    <span class="info-label">Database Disk Size</span>
                    <span class="info-val">{dbSize}</span>
                  </div>
                  <div class="info-row">
                    <span class="info-label">Local Host Port</span>
                    <span class="info-val">8758</span>
                  </div>
                </div>
              </div>
            </div>

            <div class="card glass flex-1">
              <div class="card-header">
                <h2>Developer Console</h2>
              </div>
              <div class="card-body terminal-panel">
                <div class="terminal-header">
                  <span class="terminal-dot red"></span>
                  <span class="terminal-dot yellow"></span>
                  <span class="terminal-dot green"></span>
                  <span class="terminal-title">quick diagnostics</span>
                </div>
                <div class="terminal-body" style="max-height: 180px; overflow-y: auto;">
                  {#each diagnosticLogs as log (log.time)}
                    <div class="log-line" in:fly={{ y: 10, duration: 200 }}>
                      <span class="text-muted">[{log.time}]</span>
                      <span class={log.type ? `text-${log.type}` : ''}>{log.text}</span>
                    </div>
                  {/each}
                </div>
              </div>
            </div>
          </div>
        </div>
      {/if}

      <!-- Collections Tab -->
      {#if currentTab === 'collections'}
        <div class="tab-panel active">
          <div class="split-layout">
            <div class="list-side panel-sidebar card glass">
              <div class="card-header side-header">
                <h3>Collections</h3>
              </div>
              <div class="card-body scroll-y">
                {#if collections.length === 0}
                  <div class="empty-state">No collections found.</div>
                {:else}
                  {#each collections as col (col)}
                    <button class="sidebar-list-item {selectedCollection === col ? 'active' : ''} {recentlyChanged.has(col) ? 'flash-highlight' : ''}" on:click={() => selectCollection(col)}
                      animate:flip={{ duration: 250 }}
                      in:fly={{ x: -20, duration: 250 }}
                      out:fade={{ duration: 150 }}>
                      <span class="item-name">{col}</span>
                      <span class="item-count-badge">coll</span>
                    </button>
                  {/each}
                {/if}
              </div>
            </div>

            <div class="inspect-side panel-main card glass">
              <div class="card-header flex-header">
                <h3>{selectedCollection ? selectedCollection : 'Select a collection'}</h3>
                <div class="header-actions">
                  <button class="btn btn-primary" on:click={openCreateObjectModal} disabled={!selectedCollection}>
                    Add Memory Object
                  </button>
                </div>
              </div>
              <div class="card-body flex-column scroll-y">
                <div class="scroll-table-wrapper">
                  <table class="data-table">
                    <thead>
                      <tr>
                        <th>Object ID</th>
                        <th>Text Content</th>
                        <th>Custom Data</th>
                        <th>Created At</th>
                        <th>Actions</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#if !selectedCollection}
                        <tr>
                          <td colspan="5" class="text-center text-muted">Please select a collection from the sidebar to inspect items.</td>
                        </tr>
                      {:else if objects.length === 0}
                        <tr>
                          <td colspan="5" class="text-center text-muted">This collection is currently empty.</td>
                        </tr>
                      {:else}
                        {#each objects as obj (obj.id)}
                          <tr class="{recentlyChanged.has(obj.id) ? 'flash-highlight' : ''}"
                            animate:flip={{ duration: 300 }}
                            in:fly={{ y: 20, duration: 300 }}
                            out:fade={{ duration: 150 }}>
                            <td class="code-cell font-weight-bold" title={obj.id}>{obj.id}</td>
                            <td class="text-cell" title={obj.text || ''}>{obj.text || '-'}</td>
                            <td class="code-cell" title={JSON.stringify(obj)}>{JSON.stringify(obj)}</td>
                            <td class="text-muted" style="font-size: 11px;">{obj.createdAt ? new Date(obj.createdAt).toLocaleString() : '-'}</td>
                            <td>
                              <div class="btn-group gap-sm">
                                <button class="btn btn-sm btn-secondary" on:click={() => openEditObjectModal(obj)}>Edit</button>
                                <button class="btn btn-sm btn-danger" on:click={() => deleteObject(obj.id)}>Delete</button>
                              </div>
                            </td>
                          </tr>
                        {/each}
                      {/if}
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          </div>
        </div>
      {/if}

      <!-- Events Tab -->
      {#if currentTab === 'events'}
        <div class="tab-panel active">
          <div class="split-layout">
            <div class="list-side panel-sidebar card glass">
              <div class="card-header">
                <h3>Event Streams</h3>
              </div>
              <div class="card-body scroll-y">
                {#if streams.length === 0}
                  <div class="empty-state">No event streams found.</div>
                {:else}
                  {#each streams as str (str)}
                    <button class="sidebar-list-item {selectedStream === str ? 'active' : ''} {recentlyChanged.has(str) ? 'flash-highlight' : ''}" on:click={() => selectStream(str)}
                      animate:flip={{ duration: 250 }}
                      in:fly={{ x: -20, duration: 250 }}
                      out:fade={{ duration: 150 }}>
                      <span class="item-name">{str}</span>
                      <span class="item-count-badge">stream</span>
                    </button>
                  {/each}
                {/if}
              </div>
            </div>

            <div class="inspect-side panel-main card glass">
              <div class="card-header flex-header">
                <h3>{selectedStream ? `Timeline: ${selectedStream}` : 'All Event Timelines'}</h3>
                <div class="header-actions">
                  <button class="btn btn-primary" on:click={openAppendEventModal}>
                    Append Event
                  </button>
                </div>
              </div>
              <div class="card-body scroll-y timeline-viewport">
                <div class="timeline">
                  {#if !selectedStream}
                    <div class="timeline-empty">Please choose a stream from the sidebar.</div>
                  {:else if events.length === 0}
                    <div class="timeline-empty">This stream has no logged events yet.</div>
                  {:else}
                    {#each events as ev (ev.id)}
                      <div class="timeline-item {recentlyChanged.has(ev.id) ? 'flash-highlight' : ''}"
                        animate:flip={{ duration: 300 }}
                        in:fly={{ y: 20, duration: 300 }}
                        out:fade={{ duration: 150 }}>
                        <div class="timeline-marker"></div>
                        <div class="timeline-card">
                          <div class="timeline-header">
                            <div class="timeline-title">
                              <span class="timeline-badge">{ev.type}</span>
                              <span class="text-dark font-weight-bold" style="font-size: 11px;">#{ev.id}</span>
                            </div>
                            <span class="timeline-time">{ev.createdAt ? new Date(ev.createdAt).toLocaleString() : ''}</span>
                          </div>
                          <div class="timeline-text">{ev.text || 'Logged message.'}</div>
                          {#if Object.keys(ev).filter(k => !['id','type','text','createdAt','stream'].includes(k)).length > 0}
                            <pre class="timeline-data scroll-x">{JSON.stringify(Object.fromEntries(Object.entries(ev).filter(([k]) => !['id','type','text','createdAt','stream'].includes(k))), null, 2)}</pre>
                          {/if}
                        </div>
                      </div>
                    {/each}
                  {/if}
                </div>
              </div>
            </div>
          </div>
        </div>
      {/if}

      <!-- Queues Tab -->
      {#if currentTab === 'queues'}
        <div class="tab-panel active">
          <div class="split-layout">
            <div class="list-side panel-sidebar card glass">
              <div class="card-header">
                <h3>Queues</h3>
              </div>
              <div class="card-body scroll-y">
                {#if queues.length === 0}
                  <div class="empty-state">No active queues found.</div>
                {:else}
                  {#each queues as q (q)}
                    <button class="sidebar-list-item {selectedQueue === q ? 'active' : ''} {recentlyChanged.has(q) ? 'flash-highlight' : ''}" on:click={() => selectQueue(q)}
                      animate:flip={{ duration: 250 }}
                      in:fly={{ x: -20, duration: 250 }}
                      out:fade={{ duration: 150 }}>
                      <span class="item-name">{q}</span>
                      <span class="item-count-badge">queue</span>
                    </button>
                  {/each}
                {/if}
              </div>
            </div>

            <div class="inspect-side panel-main card glass">
              <div class="card-header flex-header flex-column-mobile">
                <div class="header-details">
                  <h3>{selectedQueue ? selectedQueue : 'Select a Queue'}</h3>
                  {#if selectedQueue}
                    <div class="queue-stats-bar">
                      <span class="badge badge-ready">Ready: {queueStats.ready}</span>
                      <span class="badge badge-leased">Leased: {queueStats.leased}</span>
                      <span class="badge badge-dead">Dead: {queueStats.dead}</span>
                    </div>
                  {/if}
                </div>
                <div class="header-actions gap-sm">
                  <button class="btn btn-secondary" on:click={openPushJobModal} disabled={!selectedQueue}>Push Job</button>
                  <button class="btn btn-secondary" on:click={openClaimJobModal} disabled={!selectedQueue}>Claim Next</button>
                </div>
              </div>

              <div class="card-body flex-column scroll-y">
                <div class="tabs-subnav">
                  <button class="subnav-item {currentQueueSubtab === 'active' ? 'active' : ''}" on:click={() => currentQueueSubtab = 'active'}>
                    Active Jobs ({activeJobs.length})
                  </button>
                  <button class="subnav-item {currentQueueSubtab === 'dead' ? 'active' : ''}" on:click={() => currentQueueSubtab = 'dead'}>
                    Dead / Poison Jobs ({deadJobs.length})
                  </button>
                </div>

                {#if currentQueueSubtab === 'active'}
                  <div class="scroll-table-wrapper">
                    <table class="data-table">
                      <thead>
                        <tr>
                          <th>Job ID</th>
                          <th>Status</th>
                          <th>Attempts</th>
                          <th>Max Attempts</th>
                          <th>Payload</th>
                          <th>Available At</th>
                          <th>Actions</th>
                        </tr>
                      </thead>
                      <tbody>
                        {#if !selectedQueue}
                          <tr>
                            <td colspan="7" class="text-center text-muted">No queue selected.</td>
                          </tr>
                        {:else if activeJobs.length === 0}
                          <tr>
                            <td colspan="7" class="text-center text-muted">No active jobs in this queue.</td>
                          </tr>
                        {:else}
                          {#each activeJobs as job (job.id)}
                            <tr class="{recentlyChanged.has(job.id) ? 'flash-highlight' : ''}"
                              animate:flip={{ duration: 300 }}
                              in:fly={{ y: 20, duration: 300 }}
                              out:fade={{ duration: 150 }}>
                              <td class="code-cell font-weight-bold" title={job.id}>{job.id}</td>
                              <td><span class="badge {job.status === 'leased' ? 'badge-leased' : 'badge-ready'}">{job.status}</span></td>
                              <td class="text-center">{job.attempts}</td>
                              <td class="text-center">{job.maxAttempts}</td>
                              <td class="code-cell" title={JSON.stringify(job.payload)}>{JSON.stringify(job.payload)}</td>
                              <td class="text-muted" style="font-size: 11px;">{job.availableAt ? new Date(job.availableAt).toLocaleString() : '-'}</td>
                              <td>
                                <button class="btn btn-sm btn-secondary" on:click={() => openJobOpsModal(job)}>Inspect Ops</button>
                              </td>
                            </tr>
                          {/each}
                        {/if}
                      </tbody>
                    </table>
                  </div>
                {:else}
                  <div class="scroll-table-wrapper">
                    <table class="data-table">
                      <thead>
                        <tr>
                          <th>Job ID</th>
                          <th>Attempts</th>
                          <th>Max Attempts</th>
                          <th>Dead At</th>
                          <th>Last Error</th>
                          <th>Payload</th>
                        </tr>
                      </thead>
                      <tbody>
                        {#if !selectedQueue}
                          <tr>
                            <td colspan="6" class="text-center text-muted">No queue selected.</td>
                          </tr>
                        {:else if deadJobs.length === 0}
                          <tr>
                            <td colspan="6" class="text-center text-muted">No poison or dead jobs reported.</td>
                          </tr>
                        {:else}
                          {#each deadJobs as job (job.id)}
                            <tr
                              animate:flip={{ duration: 300 }}
                              in:fly={{ y: 20, duration: 300 }}
                              out:fade={{ duration: 150 }}>
                              <td class="code-cell font-weight-bold" title={job.id}>{job.id}</td>
                              <td class="text-center">{job.attempts}</td>
                              <td class="text-center">{job.maxAttempts}</td>
                              <td class="text-muted" style="font-size: 11px;">{job.deadAt ? new Date(job.deadAt).toLocaleString() : '-'}</td>
                              <td class="text-danger" title={job.lastError || ''}>{job.lastError || 'None'}</td>
                              <td class="code-cell" title={JSON.stringify(job.payload)}>{JSON.stringify(job.payload)}</td>
                            </tr>
                          {/each}
                        {/if}
                      </tbody>
                    </table>
                  </div>
                {/if}
              </div>
            </div>
          </div>
        </div>
      {/if}

      <!-- Search Sandbox Tab -->
      {#if currentTab === 'search'}
        <div class="tab-panel active">
          <div class="card glass fill-height flex-column">
            <div class="card-header">
              <h2>Stemming Search Tester (FTS5 Engine)</h2>
            </div>
            <div class="card-body flex-column scroll-y">
              
              <div class="search-form-grid">
                <div class="form-group flex-3">
                  <label for="search-query">Stemmed Search Query</label>
                  <div class="search-input-group">
                    <input type="text" id="search-query" bind:value={searchQuery} placeholder="e.g. agent memory, scheduler OR heartbeat..." class="form-input search-input" on:keydown={(e) => e.key === 'Enter' && runSearch()}>
                    <button class="btn btn-primary" on:click={runSearch}>Run Search</button>
                  </div>
                </div>
                
                <div class="form-group flex-1">
                  <label for="search-limit">Limit</label>
                  <input type="number" id="search-limit" bind:value={searchLimit} class="form-input" min="1">
                </div>
              </div>

              <div class="row-flex gap-lg">
                <div class="form-group flex-1">
                  <label for="search-cols">Filter Collections (optional comma list)</label>
                  <input type="text" id="search-cols" bind:value={searchCollections} placeholder="e.g. system, schedules" class="form-input">
                </div>

                <div class="form-group flex-1">
                  <label for="search-meta">Metadata Filter JSON (optional)</label>
                  <input type="text" id="search-meta" bind:value={searchMetadata} placeholder='e.g. &#123;"status":"active"&#125;' class="form-input">
                </div>
              </div>

              <div class="search-results-section">
                <h4 class="section-title">Search Results ({searchResults.length} matching objects found)</h4>
                
                <div class="scroll-table-wrapper">
                  <table class="data-table">
                    <thead>
                      <tr>
                        <th>Collection</th>
                        <th>Object ID</th>
                        <th>Stemmed / Highlighted Text Match</th>
                        <th>Custom Metadata</th>
                        <th>Score</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#if searchResults.length === 0}
                        <tr>
                          <td colspan="5" class="text-center text-muted">Enter a search query above and click 'Run Search' to inspect FTS5 matching.</td>
                        </tr>
                      {:else}
                        {#each searchResults as res (res.id)}
                          <tr
                            animate:flip={{ duration: 300 }}
                            in:fly={{ y: 20, duration: 300 }}
                            out:fade={{ duration: 150 }}>
                            <td><span class="badge badge-leased">{res.collection}</span></td>
                            <td class="code-cell font-weight-bold" title={res.id}>{res.id}</td>
                            <!-- svelte-ignore html_unsafe_element_attribute -->
                            <td style="font-size: 13px;">{@html getHighlightedText(res.text, searchQuery)}</td>
                            <td class="code-cell" title={JSON.stringify(res)}>{JSON.stringify(res)}</td>
                            <td class="font-weight-bold text-success">{res.score ? res.score.toFixed(3) : '1.000'}</td>
                          </tr>
                        {/each}
                      {/if}
                    </tbody>
                  </table>
                </div>
              </div>

            </div>
          </div>
        </div>
      {/if}

      <!-- NLQ Query Tab -->
      {#if currentTab === 'nlq'}
        <div class="tab-panel active">
          <div class="card glass fill-height flex-column">
            <div class="card-header" style="display: flex; justify-content: space-between; align-items: center;">
              <h2>Natural Language Query</h2>
              <div style="display: flex; gap: 8px;">
                <button class="btn btn-sm btn-secondary" on:click={async () => { await fetchNlqSchema(); nlqShowSchema = !nlqShowSchema; }}>
                  {nlqShowSchema ? 'Hide' : 'Show'} Schema
                </button>
                <button class="btn btn-sm btn-secondary" on:click={() => nlqShowSettings = !nlqShowSettings}>
                  ⚙️ Settings
                </button>
              </div>
            </div>
            <div class="card-body flex-column scroll-y">

              <!-- NLQ Input -->
              <div class="search-form-grid">
                <div class="form-group flex-3">
                  <label for="nlq-question">Ask a question about your data</label>
                  <div class="search-input-group">
                    <input type="text" id="nlq-question" bind:value={nlqQuestion}
                      placeholder="e.g. total revenue by region, most recent events, count of all objects..."
                      class="form-input search-input"
                      on:keydown={(e) => e.key === 'Enter' && runNlq()}>
                    <button class="btn btn-primary" on:click={runNlq} disabled={nlqLoading}>
                      {nlqLoading ? 'Thinking...' : 'Ask'}
                    </button>
                  </div>
                </div>
                <div class="form-group flex-1">
                  <label for="nlq-collection">Collection (optional)</label>
                  <input type="text" id="nlq-collection" bind:value={nlqCollection} placeholder="e.g. orders" class="form-input">
                </div>
              </div>

              <!-- LLM Settings -->
              {#if nlqShowSettings}
                <div class="card glass" style="margin: 12px 0; padding: 16px;">
                  <h4 style="margin: 0 0 12px 0;">LLM Configuration</h4>
                  <div class="row-flex gap-lg">
                    <div class="form-group flex-1">
                      <label for="nlq-model">Model</label>
                      <input type="text" id="nlq-model" bind:value={nlqModel} placeholder="llama3" class="form-input">
                    </div>
                    <div class="form-group flex-2">
                      <label for="nlq-endpoint">Endpoint</label>
                      <input type="text" id="nlq-endpoint" bind:value={nlqEndpoint} placeholder="http://localhost:11434/v1" class="form-input">
                    </div>
                    <div class="form-group flex-1">
                      <label for="nlq-apikey">API Key (optional)</label>
                      <input type="password" id="nlq-apikey" bind:value={nlqApiKey} placeholder="sk-..." class="form-input">
                    </div>
                  </div>
                  <button class="btn btn-sm btn-primary" style="margin-top: 8px;" on:click={saveNlqSettings}>Save Settings</button>
                </div>
              {/if}

              <!-- Schema Display -->
              {#if nlqShowSchema && nlqSchema}
                <div class="card glass" style="margin: 12px 0; padding: 16px;">
                  <h4 style="margin: 0 0 12px 0;">Schema</h4>
                  {#each nlqSchema as col}
                    <div style="margin-bottom: 12px;">
                      <strong style="color: var(--color-primary);">{col.name}</strong>
                      <span class="text-muted" style="margin-left: 8px;">({col.objectCount} objects)</span>
                      <div style="margin-top: 4px; font-size: 12px;">
                        {#each col.fields as field}
                          <span class="badge badge-queued" style="margin: 2px;">
                            {field.name}: <span class="text-muted">{field.type}</span>
                          </span>
                        {/each}
                      </div>
                    </div>
                  {/each}
                </div>
              {:else if nlqShowSchema}
                <p class="text-muted" style="margin: 12px 0;">Click "Show Schema" to load schema.</p>
              {/if}

              <!-- NLQ Results -->
              {#if nlqLoading}
                <div class="search-results-section">
                  <p class="text-muted">Querying LLM and executing...</p>
                </div>
              {:else if nlqResults}
                <div class="search-results-section">
                  <div class="card glass" style="margin-bottom: 12px; padding: 16px;">
                    <h4 style="margin: 0 0 4px 0;">Answer</h4>
                    <p style="font-size: 16px;">{nlqResults.answer}</p>
                  </div>

                  <!-- Aggregate results (table) -->
                  {#if nlqResults.data && nlqResults.data.total !== undefined}
                    <div class="scroll-table-wrapper">
                      <table class="data-table">
                        <thead>
                          <tr>
                            <th>Total</th>
                            {#if nlqResults.data.groups && nlqResults.data.groups.length > 0}
                              <th>Group</th>
                              <th>Value</th>
                            {/if}
                          </tr>
                        </thead>
                        <tbody>
                          <tr>
                            <td class="font-weight-bold text-success">{formatNlqValue(nlqResults.data.total)}</td>
                            {#if nlqResults.data.groups && nlqResults.data.groups.length > 0}
                              <td colspan="2"></td>
                            {/if}
                          </tr>
                          {#if nlqResults.data.groups}
                            {#each nlqResults.data.groups as group}
                              <tr>
                                <td></td>
                                <td><span class="badge badge-leased">{group.key}</span></td>
                                <td class="font-weight-bold">{formatNlqValue(group.value)}</td>
                              </tr>
                            {/each}
                          {/if}
                        </tbody>
                      </table>
                    </div>

                  <!-- Timeseries results -->
                  {:else if nlqResults.data && nlqResults.data.buckets}
                    <div class="scroll-table-wrapper">
                      <table class="data-table">
                        <thead>
                          <tr>
                            <th>Bucket</th>
                            <th>Value</th>
                          </tr>
                        </thead>
                        <tbody>
                          {#each nlqResults.data.buckets as bucket}
                            <tr>
                              <td>{bucket.label}</td>
                              <td class="font-weight-bold">{formatNlqValue(bucket.value)}</td>
                            </tr>
                          {/each}
                        </tbody>
                      </table>
                    </div>

                  <!-- Search results (list) -->
                  {:else if nlqResults.data && Array.isArray(nlqResults.data)}
                    <div class="scroll-table-wrapper">
                      <table class="data-table">
                        <thead>
                          <tr>
                            <th>Collection</th>
                            <th>ID</th>
                            <th>Score</th>
                          </tr>
                        </thead>
                        <tbody>
                          {#each nlqResults.data as hit}
                            <tr>
                              <td><span class="badge badge-leased">{hit.collection || hit.stream}</span></td>
                              <td class="code-cell">{hit.id}</td>
                              <td class="font-weight-bold text-success">{hit.score?.toFixed(3) ?? '-'}</td>
                            </tr>
                          {/each}
                        </tbody>
                      </table>
                    </div>
                  {/if}

                  <!-- Raw result toggle -->
                  <details style="margin-top: 12px;">
                    <summary style="cursor: pointer; color: var(--text-muted); font-size: 12px;">Raw JSON</summary>
                    <pre class="code-block" style="margin-top: 8px;">{JSON.stringify(nlqResults, null, 2)}</pre>
                  </details>
                </div>
              {:else}
                <div class="search-results-section">
                  <p class="text-muted">Ask a question above to query your data using natural language.</p>
                </div>
              {/if}

            </div>
          </div>
        </div>
      {/if}

      <!-- Database Health Tab -->
      {#if currentTab === 'health'}
        <div class="tab-panel active">
          <div class="card glass fill-height flex-column">
            <div class="card-header">
              <h2>Database Health</h2>
            </div>
            <div class="card-body scroll-y">
              <div class="metrics-grid">
                <div class="metric-card glass">
                  <div class="metric-value">{dbHealth.integrityOk ? '✓' : '✗'}</div>
                  <div class="metric-label">Integrity Check</div>
                  <div class="metric-detail">{dbHealth.integrityMessage}</div>
                  <button class="btn btn-sm btn-secondary" on:click={runIntegrityCheck} style="margin-top: 8px;">
                    Run Check
                  </button>
                </div>
                <div class="metric-card glass">
                  <div class="metric-value">{dbHealth.walMode}</div>
                  <div class="metric-label">WAL Mode</div>
                  <div class="metric-detail">Frames: {dbHealth.walFrames}</div>
                  <button class="btn btn-sm btn-secondary" on:click={runWalCheckpoint} style="margin-top: 8px;">
                    Run Checkpoint
                  </button>
                </div>
                <div class="metric-card glass">
                  <div class="metric-value">{dbHealth.backupPath || 'N/A'}</div>
                  <div class="metric-label">Last Backup</div>
                  <div class="metric-detail">
                    {#if dbHealth.backupSize}<span>{(dbHealth.backupSize / 1024 / 1024).toFixed(2)} MB</span>{/if}
                  </div>
                  <button class="btn btn-sm btn-secondary" on:click={createBackup} style="margin-top: 8px;">
                    Create Backup
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      {/if}

    </div>
  </main>
</div>

<!-- Modal Overlays -->

<!-- Object Modal -->
{#if modalObjectVisible}
  <div class="modal-overlay active">
    <div class="modal-card">
      <div class="modal-header">
        <h3>{editObjectMode ? 'Edit Memory Object' : 'Add Memory Object'}</h3>
        <button class="modal-close-btn" on:click={() => modalObjectVisible = false}>&times;</button>
      </div>
      <div class="modal-body">
        <form on:submit={saveObject}>
          <div class="form-group">
            <label for="obj-id">Object ID *</label>
            <input type="text" id="obj-id" bind:value={objId} placeholder="e.g. config-123" class="form-input" required disabled={editObjectMode}>
          </div>
          <div class="form-group">
            <label for="obj-text">Text Content (indexed for stemming search)</label>
            <textarea id="obj-text" bind:value={objText} placeholder="Enter text..." class="form-input" rows="4"></textarea>
          </div>
          <div class="form-group">
            <JsonEditor bind:value={objData} label="Custom Metadata" placeholder={'{"status": "active"}'} />
          </div>
          <div class="modal-footer">
            <button type="button" class="btn btn-secondary" on:click={() => modalObjectVisible = false}>Cancel</button>
            <button type="submit" class="btn btn-primary">Save Object</button>
          </div>
        </form>
      </div>
    </div>
  </div>
{/if}

<!-- Event Modal -->
{#if modalEventVisible}
  <div class="modal-overlay active">
    <div class="modal-card">
      <div class="modal-header">
        <h3>Append Memory Event</h3>
        <button class="modal-close-btn" on:click={() => modalEventVisible = false}>&times;</button>
      </div>
      <div class="modal-body">
        <form on:submit={appendEvent}>
          <div class="form-group">
            <label for="ev-stream">Stream Name *</label>
            <input type="text" id="ev-stream" bind:value={eventStream} placeholder="e.g. logs" class="form-input" required>
          </div>
          <div class="form-group">
            <label for="ev-type">Event Type *</label>
            <input type="text" id="ev-type" bind:value={eventType} placeholder="e.g. decision" class="form-input" required>
          </div>
          <div class="form-group">
            <label for="ev-text">Event Text</label>
            <input type="text" id="ev-text" bind:value={eventText} placeholder="Enter message..." class="form-input">
          </div>
          <div class="form-group">
            <JsonEditor bind:value={eventData} label="Event Payload" placeholder={'{"success": true}'} />
          </div>
          <div class="modal-footer">
            <button type="button" class="btn btn-secondary" on:click={() => modalEventVisible = false}>Cancel</button>
            <button type="submit" class="btn btn-primary">Append Event</button>
          </div>
        </form>
      </div>
    </div>
  </div>
{/if}

<!-- Push Job Modal -->
{#if modalPushJobVisible}
  <div class="modal-overlay active">
    <div class="modal-card">
      <div class="modal-header">
        <h3>Push Job onto Queue</h3>
        <button class="modal-close-btn" on:click={() => modalPushJobVisible = false}>&times;</button>
      </div>
      <div class="modal-body">
        <form on:submit={pushJob}>
          <div class="form-group">
            <JsonEditor bind:value={jobPayload} label="Payload" placeholder={'{"action": "build"}'} required={true} />
          </div>
          <div class="row-flex gap-md">
            <div class="form-group flex-1">
              <label for="job-delay">Delay (ms)</label>
              <input type="number" id="job-delay" bind:value={jobDelay} placeholder="0" class="form-input" min="0">
            </div>
            <div class="form-group flex-1">
              <label for="job-attempts">Max Attempts</label>
              <input type="number" id="job-attempts" bind:value={jobAttempts} class="form-input" min="1">
            </div>
          </div>
          <div class="form-group">
            <label for="job-idempotency">Idempotency Key (optional)</label>
            <input type="text" id="job-idempotency" bind:value={jobIdempotency} placeholder="e.g. lock-1" class="form-input">
          </div>
          <div class="modal-footer">
            <button type="button" class="btn btn-secondary" on:click={() => modalPushJobVisible = false}>Cancel</button>
            <button type="submit" class="btn btn-primary">Push Job</button>
          </div>
        </form>
      </div>
    </div>
  </div>
{/if}

<!-- Claim Job Modal -->
{#if modalClaimJobVisible}
  <div class="modal-overlay active">
    <div class="modal-card">
      <div class="modal-header">
        <h3>Claim Next Queue Job</h3>
        <button class="modal-close-btn" on:click={() => modalClaimJobVisible = false}>&times;</button>
      </div>
      <div class="modal-body">
        <form on:submit={claimJob}>
          <div class="form-group">
            <label for="claim-lease">Lease Duration (ms)</label>
            <input type="number" id="claim-lease" bind:value={claimLease} placeholder="60000" class="form-input" min="1000">
            <span class="input-tip">Lock period in ms.</span>
          </div>
          <div class="modal-footer">
            <button type="button" class="btn btn-secondary" on:click={() => modalClaimJobVisible = false}>Cancel</button>
            <button type="submit" class="btn btn-primary">Claim Job</button>
          </div>
        </form>
      </div>
    </div>
  </div>
{/if}

<!-- Connection Settings Modal -->
{#if modalConnVisible}
  <div class="modal-overlay active">
    <div class="modal-card">
      <div class="modal-header">
        <h3>Connection Settings</h3>
        <button class="modal-close-btn" on:click={() => modalConnVisible = false}>&times;</button>
      </div>
      <div class="modal-body">
        <form on:submit={handleConnSwitch}>
          <div class="form-group">
            <label for="conn-drv">Storage Driver *</label>
            <select id="conn-drv" bind:value={connDriver} class="form-input" style="background: rgba(0, 0, 0, 0.4);">
              <option value="memory">In-Memory Store</option>
              <option value="native">Native SQLite (FTS5)</option>
              <option value="cloud">Cloud Endpoint</option>
            </select>
          </div>
          <div class="form-group">
            <label for="conn-pth">Database Path / Endpoint URL *</label>
            <input type="text" id="conn-pth" bind:value={connPath} placeholder="e.g. :memory: or database.db" class="form-input" required>
          </div>
          <div class="form-group">
            <label for="conn-tok">Bearer Security Token (optional)</label>
            <input type="password" id="conn-tok" bind:value={connToken} placeholder="Security authorization key..." class="form-input">
          </div>
          <div class="modal-footer">
            <button type="button" class="btn btn-secondary" on:click={() => modalConnVisible = false}>Cancel</button>
            <button type="submit" class="btn btn-primary">Connect</button>
          </div>
        </form>
      </div>
    </div>
  </div>
{/if}

<!-- Job Ops Supervisor Modal -->
{#if modalJobOpsVisible && selectedJob}
  <div class="modal-overlay active">
    <div class="modal-card wide">
      <div class="modal-header">
        <h3>Queue Job Operations</h3>
        <button class="modal-close-btn" on:click={() => modalJobOpsVisible = false}>&times;</button>
      </div>
      <div class="modal-body flex-column">
        <div class="job-detail-card">
          <div class="info-row">
            <span class="info-label">Job ID</span>
            <span class="info-val code">{selectedJob.id}</span>
          </div>
          <div class="info-row">
            <span class="info-label">Queue</span>
            <span class="info-val code">{selectedQueue}</span>
          </div>
          <div class="info-row">
            <span class="info-label">Status</span>
            <span class="info-val">{selectedJob.status}</span>
          </div>
          <div class="info-row">
            <span class="info-label">Attempts</span>
            <span class="info-val">{selectedJob.attempts} of {selectedJob.maxAttempts}</span>
          </div>
          <div class="info-group">
            <label for="payload-ops-view">Payload JSON</label>
            <pre id="payload-ops-view" class="code-view block scroll-x">{JSON.stringify(selectedJob.payload, null, 2)}</pre>
          </div>
        </div>

        {#if selectedJob.status === 'leased'}
          <div class="ops-controls">
            <h4 class="ops-section-title">Ack / Nack Supervisor (Resolve Job)</h4>
            <div class="btn-group gap-md" style="display: flex;">
              <button class="btn btn-success flex-1" on:click={ackJob}>Acknowledge Job (Ack)</button>
              <button class="btn btn-warning flex-1" on:click={() => nackFormVisible = !nackFormVisible}>Reject / Fail Job (Nack)</button>
            </div>
            
            {#if nackFormVisible}
              <form on:submit={nackJob} class="nack-details-form form-group mt-md">
                <div class="form-group">
                  <label for="nack-err">Error Message (optional)</label>
                  <input type="text" id="nack-err" bind:value={nackError} placeholder="e.g. failure..." class="form-input">
                </div>
                <div class="form-group">
                  <label for="nack-del">Retry Delay (ms)</label>
                  <input type="number" id="nack-del" bind:value={nackDelay} class="form-input">
                </div>
                <button type="submit" class="btn btn-primary">Submit Nack</button>
              </form>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<!-- Toast List -->
<div class="toast-container" id="toast-container">
  {#each toasts as t}
    <div class="toast {t.type}">
      <div class="toast-message">{t.message}</div>
    </div>
  {/each}
</div>

<style>
  /* Local overrides if needed */
</style>
