<script>
  import { onMount } from 'svelte';

  export let value = '';
  export let placeholder = 'e.g. {"status": "active"}';
  export let label = '';
  export let required = false;

  let activeTab = 'raw'; // 'raw' or 'visual'
  let textValue = value || '';
  let isValid = true;
  let validationError = '';
  
  // Rows for visual builder:
  // { id: string, key: string, val: any, type: 'string' | 'number' | 'boolean' | 'null' | 'json' }
  let rows = [];

  // Track outer changes
  $: if (value !== textValue) {
    textValue = value || '';
    validateAndSyncRows();
  }

  function generateId() {
    return Math.random().toString(36).substring(2, 9);
  }

  function validateAndSyncRows() {
    if (!textValue.trim()) {
      isValid = true;
      validationError = '';
      rows = [];
      return;
    }

    try {
      const parsed = JSON.parse(textValue);
      isValid = true;
      validationError = '';
      
      // If it's a valid object, synchronize the visual builder rows
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        rows = Object.entries(parsed).map(([key, val]) => {
          let type = 'string';
          let displayVal = val;
          
          if (val === null) {
            type = 'null';
            displayVal = '';
          } else if (typeof val === 'number') {
            type = 'number';
          } else if (typeof val === 'boolean') {
            type = 'boolean';
          } else if (typeof val === 'object') {
            type = 'json';
            displayVal = JSON.stringify(val);
          }
          
          return { id: generateId(), key, val: displayVal, type };
        });
      } else {
        // Valid JSON but not a flat object
        rows = [];
      }
    } catch (err) {
      isValid = false;
      validationError = err.message;
    }
  }

  function handleTextChange() {
    value = textValue;
    validateAndSyncRows();
  }

  function prettifyJson() {
    if (!textValue.trim()) return;
    try {
      const parsed = JSON.parse(textValue);
      textValue = JSON.stringify(parsed, null, 2);
      value = textValue;
      isValid = true;
      validationError = '';
      validateAndSyncRows();
    } catch (err) {
      isValid = false;
      validationError = err.message;
    }
  }

  function syncRowsToValue() {
    const obj = {};
    for (const row of rows) {
      if (!row.key.trim()) continue; // skip empty keys
      
      let parsedVal;
      if (row.type === 'number') {
        parsedVal = row.val === '' ? 0 : Number(row.val);
      } else if (row.type === 'boolean') {
        parsedVal = row.val === true || row.val === 'true';
      } else if (row.type === 'null') {
        parsedVal = null;
      } else if (row.type === 'json') {
        try {
          parsedVal = JSON.parse(row.val);
        } catch (e) {
          parsedVal = row.val; // fallback if invalid
        }
      } else {
        parsedVal = row.val;
      }
      obj[row.key] = parsedVal;
    }

    textValue = JSON.stringify(obj, null, 2);
    value = textValue;
    isValid = true;
    validationError = '';
  }

  function addRow() {
    rows = [...rows, { id: generateId(), key: '', val: '', type: 'string' }];
  }

  function removeRow(id) {
    rows = rows.filter(r => r.id !== id);
    syncRowsToValue();
  }

  function handleRowChange() {
    syncRowsToValue();
  }

  function handleTypeChange(row, event) {
    const newType = event.target.value;
    row.type = newType;
    if (newType === 'boolean') {
      row.val = false;
    } else if (newType === 'null') {
      row.val = '';
    } else if (newType === 'number') {
      row.val = 0;
    } else if (newType === 'json') {
      row.val = '{}';
    } else {
      row.val = '';
    }
    syncRowsToValue();
  }

  function switchTab(tab) {
    activeTab = tab;
    if (tab === 'visual') {
      validateAndSyncRows();
    }
  }

  function resetToEmptyObject() {
    textValue = '{}';
    value = '{}';
    isValid = true;
    validationError = '';
    rows = [];
    activeTab = 'visual';
    validateAndSyncRows();
  }

  onMount(() => {
    validateAndSyncRows();
  });
</script>

<div class="json-editor-container">
  <div class="json-editor-header">
    {#if label}
      <span class="json-editor-label">{label} {required ? '*' : ''}</span>
    {/if}
    <div class="json-editor-tabs">
      <button 
        type="button" 
        class="json-tab-btn {activeTab === 'raw' ? 'active' : ''}" 
        on:click={() => switchTab('raw')}
      >
        <svg viewBox="0 0 24 24" class="tab-icon"><path d="M9.4 16.6L4.8 12l4.6-4.6L8 6l-6 6 6 6 1.4-1.4zm5.2 0l4.6-4.6-4.6-4.6L16 6l6 6-6 6-1.4-1.4z"/></svg>
        Raw JSON
      </button>
      <button 
        type="button" 
        class="json-tab-btn {activeTab === 'visual' ? 'active' : ''}" 
        on:click={() => switchTab('visual')}
      >
        <svg viewBox="0 0 24 24" class="tab-icon"><path d="M4 14h6v-4H4v4zm0 5h6v-4H4v4zM4 9h6V5H4v4zm10 5h6v-4h-6v4zm0 5h6v-4h-6v4zM14 5v4h6V5h-6z"/></svg>
        Visual Builder
      </button>
    </div>
  </div>

  <div class="json-editor-content">
    {#if activeTab === 'raw'}
      <div class="raw-editor-view">
        <textarea
          class="form-input code raw-textarea {!isValid ? 'invalid-border' : ''}"
          bind:value={textValue}
          {placeholder}
          rows="5"
          {required}
          on:input={handleTextChange}
        ></textarea>
        
        <div class="json-editor-status-bar">
          {#if isValid}
            <div class="status-badge valid">
              <span class="status-dot"></span>
              Valid JSON
            </div>
          {:else}
            <div class="status-badge invalid" title={validationError}>
              <span class="status-dot"></span>
              Malformed JSON
            </div>
          {/if}
          
          <div class="status-actions">
            <button type="button" class="btn btn-secondary btn-sm" on:click={prettifyJson}>
              <svg viewBox="0 0 24 24" width="12" height="12" style="fill: currentColor; margin-right: 4px;"><path d="M4 6h16v2H4zm0 5h16v2H4zm0 5h16v2H4z"/></svg>
              Format
            </button>
          </div>
        </div>
        
        {#if !isValid && validationError}
          <div class="error-msg-banner">
            {validationError}
          </div>
        {/if}
      </div>
    {:else}
      <div class="visual-editor-view">
        {#if !isValid}
          <div class="error-visual-fallback card">
            <div class="card-body text-center">
              <svg viewBox="0 0 24 24" width="32" height="32" class="text-danger" style="margin-bottom: 12px; fill: currentColor;"><path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/></svg>
              <h4>Cannot Parse Object</h4>
              <p class="text-muted" style="font-size: 12px; margin: 8px 0 16px;">The current text has formatting errors or is not a valid JSON object. Please fix it in the Raw JSON tab or reset to an empty object.</p>
              <button type="button" class="btn btn-primary btn-sm" on:click={resetToEmptyObject}>Reset to Empty Object</button>
            </div>
          </div>
        {:else}
          <div class="visual-rows-container">
            {#if rows.length === 0}
              <div class="empty-rows-state">
                No custom key-value attributes defined. Click "Add Property" to build your JSON.
              </div>
            {:else}
              <div class="rows-header-grid">
                <span class="header-col">Property Name</span>
                <span class="header-col">Type</span>
                <span class="header-col">Value</span>
                <span class="header-col action-col"></span>
              </div>
              <div class="rows-list">
                {#each rows as row (row.id)}
                  <div class="row-builder-item">
                    <input 
                      type="text" 
                      placeholder="key" 
                      class="form-input key-input" 
                      bind:value={row.key} 
                      on:input={handleRowChange}
                    />
                    
                    <select 
                      class="form-input type-select" 
                      value={row.type} 
                      on:change={(e) => handleTypeChange(row, e)}
                    >
                      <option value="string">String</option>
                      <option value="number">Number</option>
                      <option value="boolean">Boolean</option>
                      <option value="null">Null</option>
                      <option value="json">JSON / Object</option>
                    </select>

                    <div class="value-input-wrapper">
                      {#if row.type === 'boolean'}
                        <select 
                          class="form-input value-select" 
                          bind:value={row.val} 
                          on:change={handleRowChange}
                        >
                          <option value={false}>False</option>
                          <option value={true}>True</option>
                        </select>
                      {:else if row.type === 'null'}
                        <div class="null-placeholder">null</div>
                      {:else if row.type === 'number'}
                        <input 
                          type="number" 
                          class="form-input val-input" 
                          bind:value={row.val} 
                          on:input={handleRowChange}
                        />
                      {:else if row.type === 'json'}
                        <textarea 
                          rows="1" 
                          class="form-input val-textarea code" 
                          placeholder="e.g. &#123;&quot;nested&quot;: 1&#125;"
                          bind:value={row.val} 
                          on:input={handleRowChange}
                        ></textarea>
                      {:else}
                        <input 
                          type="text" 
                          placeholder="value" 
                          class="form-input val-input" 
                          bind:value={row.val} 
                          on:input={handleRowChange}
                        />
                      {/if}
                    </div>

                    <button 
                      type="button" 
                      class="btn btn-secondary btn-icon remove-row-btn" 
                      on:click={() => removeRow(row.id)}
                      title="Remove Row"
                    >
                      <svg viewBox="0 0 24 24" width="14" height="14" style="fill: currentColor;"><path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12 19 6.41z"/></svg>
                    </button>
                  </div>
                {/each}
              </div>
            {/if}

            <div class="visual-actions-footer">
              <button type="button" class="btn btn-secondary btn-sm" on:click={addRow}>
                <svg viewBox="0 0 24 24" width="14" height="14" style="fill: currentColor; margin-right: 4px;"><path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z"/></svg>
                Add Property
              </button>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .json-editor-container {
    display: flex;
    flex-direction: column;
    width: 100%;
    margin-bottom: 8px;
  }

  .json-editor-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .json-editor-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .json-editor-tabs {
    display: flex;
    gap: 4px;
    background: rgba(0, 0, 0, 0.25);
    padding: 2px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--glass-border);
  }

  .json-tab-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    border-radius: 4px;
    transition: all 0.2s ease;
  }

  .json-tab-btn:hover {
    color: var(--text-main);
    background: rgba(255, 255, 255, 0.03);
  }

  .json-tab-btn.active {
    background: rgba(99, 102, 241, 0.15);
    color: white;
    box-shadow: inset 0 0 0 1px rgba(99, 102, 241, 0.25);
  }

  .tab-icon {
    width: 12px;
    height: 12px;
    fill: currentColor;
  }

  .json-editor-content {
    width: 100%;
  }

  .raw-editor-view {
    display: flex;
    flex-direction: column;
    width: 100%;
  }

  .raw-textarea {
    width: 100%;
    resize: vertical;
    min-height: 110px;
  }

  .raw-textarea.invalid-border {
    border-color: rgba(239, 68, 68, 0.5);
  }

  .raw-textarea.invalid-border:focus {
    border-color: var(--color-danger);
    box-shadow: 0 0 0 3px var(--color-danger-glow);
  }

  .json-editor-status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 6px;
    padding: 0 2px;
  }

  .status-badge {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 600;
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .status-badge.valid {
    color: var(--color-success);
  }

  .status-badge.valid .status-dot {
    background-color: var(--color-success);
    box-shadow: 0 0 6px var(--color-success);
  }

  .status-badge.invalid {
    color: var(--color-danger);
    cursor: help;
  }

  .status-badge.invalid .status-dot {
    background-color: var(--color-danger);
    box-shadow: 0 0 6px var(--color-danger);
  }

  .error-msg-banner {
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.15);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
    color: #fca5a5;
    font-size: 11.5px;
    font-family: var(--font-mono);
    margin-top: 8px;
    line-height: 1.4;
    word-break: break-all;
  }

  .visual-editor-view {
    width: 100%;
  }

  .error-visual-fallback {
    background: rgba(239, 68, 68, 0.03);
    border-color: rgba(239, 68, 68, 0.1);
  }

  .visual-rows-container {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: rgba(0, 0, 0, 0.15);
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-sm);
    padding: 12px;
    max-height: 280px;
    overflow-y: auto;
  }

  .empty-rows-state {
    text-align: center;
    padding: 24px 12px;
    color: var(--text-muted);
    font-size: 12px;
  }

  .rows-header-grid {
    display: grid;
    grid-template-columns: 140px 100px 1fr 34px;
    gap: 8px;
    padding: 0 4px 6px;
    border-bottom: 1px solid var(--glass-border);
  }

  .header-col {
    font-size: 10px;
    font-weight: 700;
    color: var(--text-dark);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .rows-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
  }

  .row-builder-item {
    display: grid;
    grid-template-columns: 140px 100px 1fr 34px;
    gap: 8px;
    align-items: center;
  }

  .row-builder-item :global(.form-input) {
    padding: 6px 10px;
    font-size: 12.5px;
  }

  .key-input {
    font-family: var(--font-mono);
  }

  .type-select {
    cursor: pointer;
  }

  .value-input-wrapper {
    width: 100%;
    display: flex;
  }

  .val-input, .value-select {
    width: 100%;
  }

  .null-placeholder {
    width: 100%;
    padding: 6px 10px;
    color: var(--text-dark);
    font-style: italic;
    font-size: 12.5px;
    background: rgba(0, 0, 0, 0.15);
    border: 1px dashed var(--glass-border);
    border-radius: var(--radius-sm);
  }

  .val-textarea {
    width: 100%;
    resize: vertical;
    min-height: 31px;
    padding-top: 6px;
    padding-bottom: 6px;
  }

  .remove-row-btn {
    width: 32px;
    height: 32px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
  }

  .remove-row-btn:hover {
    background: var(--color-danger-glow);
    border-color: rgba(239, 68, 68, 0.3);
    color: var(--color-danger);
  }

  .visual-actions-footer {
    display: flex;
    justify-content: flex-start;
    margin-top: 10px;
    border-top: 1px solid rgba(255, 255, 255, 0.04);
    padding-top: 10px;
  }
</style>
