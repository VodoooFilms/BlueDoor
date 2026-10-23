<script lang="ts">
  import Icon from './Icon.svelte';
  import { tick, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  type Kind = 'document' | 'audio' | 'map' | 'folder';
  type Entry = {
    id: string;
    title: string;
    kind: Kind;
    mime: string;
    size: number;
    createdAt: string;
    description: string;
    sha256: string;
  };
  type Status = {
    initialized: boolean;
    unlocked: boolean;
    entryCount: number;
    totalBytes: number;
    autoLockSeconds: number;
    createdAt: string | null;
    fileVault: boolean | null;
    hardwareProtection: boolean;
  };

  const kinds: { id: Kind; label: string; icon: string }[] = [
    { id: 'document', label: 'Documento', icon: 'document' },
    { id: 'audio', label: 'Transcripción', icon: 'audio' },
    { id: 'folder', label: 'Original', icon: 'folder' },
    { id: 'map', label: 'Mapa de alias', icon: 'map' },
  ];
  const categories = [
    { id: 'all', label: 'Todo el Vault', icon: 'grid' },
    { id: 'document', label: 'Documentos', icon: 'document' },
    { id: 'audio', label: 'Transcripciones', icon: 'audio' },
    { id: 'folder', label: 'Originales', icon: 'folder' },
    { id: 'map', label: 'Mapas de alias', icon: 'map' },
  ];

  let screen = $state<'library' | 'locked' | 'setup' | 'security'>('locked');
  let ready = $state(false);
  let category = $state('all');
  let query = $state('');
  let layout = $state<'grid' | 'list'>('grid');
  let entries = $state<Entry[]>([]);
  let status = $state<Status | null>(null);
  let selected = $state<Entry | null>(null);
  let sort = $state('recent');
  let setupStep = $state(1);
  let recoverySaved = $state(false);
  let recoveryPhrase = $state('');
  let password = $state('');
  let confirmation = $state('');
  let restoreSecret = $state('');
  let replaceExisting = $state(false);
  let passwordError = $state('');
  let formError = $state('');
  let busy = $state(false);
  let notice = $state('');
  let dialog: HTMLDialogElement;
  let mainEl = $state<HTMLElement>();
  let modalTrigger: HTMLElement | null = null;
  let modal = $state<'import' | 'export' | 'delete' | 'recovery' | 'migrate' | 'restore' | 'hardware'>('import');
  let lastActivity = Date.now();

  const visible = $derived(
    entries
      .filter((entry) => (category === 'all' || entry.kind === category) && entry.title.toLocaleLowerCase().includes(query.toLocaleLowerCase()))
      .sort((a, b) => (sort === 'name' ? a.title.localeCompare(b.title) : b.createdAt.localeCompare(a.createdAt))),
  );
  const count = (id: string) => entries.filter((entry) => id === 'all' || entry.kind === id).length;
  const title = $derived(categories.find((item) => item.id === category)?.label ?? 'Todo el Vault');
  const words = $derived(recoveryPhrase ? recoveryPhrase.split(' ').filter(Boolean) : []);

  function formatBytes(bytes: number) {
    if (!bytes) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    const index = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
    const value = bytes / 1024 ** index;
    return `${new Intl.NumberFormat('es-ES', { maximumFractionDigits: index === 0 ? 0 : 1 }).format(value)} ${units[index]}`;
  }
  function formatDate(iso: string) {
    const date = new Date(iso);
    return Number.isNaN(date.getTime()) ? iso : date.toLocaleDateString('es-ES', { day: 'numeric', month: 'short', year: 'numeric' });
  }
  function kindLabel(kind: Kind) {
    return kinds.find((item) => item.id === kind)?.label ?? 'Documento';
  }
  function errorText(error: unknown) {
    if (typeof error === 'string') return error;
    if (error && typeof error === 'object' && 'message' in error) return String((error as { message: unknown }).message);
    return 'No se pudo completar la operación.';
  }
  function touch() {
    lastActivity = Date.now();
  }
  async function run<T>(action: () => Promise<T>): Promise<T | null> {
    busy = true;
    formError = '';
    try {
      return await action();
    } catch (error) {
      formError = errorText(error);
      // Fuera de un diálogo no hay dónde mostrar `formError`; el aviso de la
      // biblioteca evita que un fallo de exportación pase en silencio.
      if (!dialog?.open) notice = formError;
      return null;
    } finally {
      busy = false;
      touch();
    }
  }

  async function refreshStatus() {
    const next = await invoke<Status>('vault_status');
    status = next;
    if (!next.initialized) screen = 'setup';
    else if (!next.unlocked) screen = 'locked';
    return next;
  }
  async function loadEntries() {
    entries = await invoke<Entry[]>('vault_list');
    selected = null;
  }
  // Refresca el estado y la lista tras una operación que cambia el Vault:
  // el tamaño total y el recuento los calcula el estado nativo.
  async function reload() {
    try {
      const next = await invoke<Status>('vault_status');
      status = next;
      await loadEntries();
    } catch (error) {
      notice = errorText(error);
    }
  }
  async function enterVault() {
    await refreshStatus();
    if (status?.unlocked) {
      await loadEntries();
      screen = 'library';
    }
  }

  onMount(() => {
    void (async () => {
      try {
        await enterVault();
        ready = true;
      } catch (error) {
        formError = errorText(error);
        screen = 'locked';
        ready = true;
      }
    })();
  });

  // Bloqueo por inactividad: la comprobación real vive también en Rust; este
  // temporizador solo adelanta la interfaz y evita seguir mostrando títulos
  // cuando la sesión ya no es válida.
  $effect(() => {
    const timer = setInterval(() => {
      if (busy) return;
      if (screen !== 'library' && screen !== 'security') return;
      const limit = (status?.autoLockSeconds ?? 300) * 1000;
      if (Date.now() - lastActivity >= limit) {
        void lockVault();
      }
    }, 5000);
    return () => clearInterval(timer);
  });

  function changeScreen(next: typeof screen) {
    screen = next;
    selected = null;
    notice = '';
    password = '';
    confirmation = '';
    formError = '';
    touch();
    void tick().then(() => mainEl?.scrollTo({ top: 0 }));
  }
  async function openModal(next: typeof modal, trigger?: EventTarget | null) {
    modalTrigger = trigger instanceof HTMLElement ? trigger : document.activeElement instanceof HTMLElement ? document.activeElement : null;
    modal = next;
    notice = '';
    formError = '';
    restoreSecret = '';
    replaceExisting = false;
    await tick();
    dialog.showModal();
  }
  function closeModal() {
    dialog.close();
  }
  function finish(message: string) {
    closeModal();
    notice = message;
  }

  async function createVault() {
    if (password.length < 8) {
      passwordError = 'Usa al menos 8 caracteres.';
      return;
    }
    if (password !== confirmation) {
      passwordError = 'Las dos contraseñas no coinciden.';
      return;
    }
    passwordError = '';
    const created = await run(() => invoke<{ recoveryPhrase: string; status: Status }>('vault_create', { password }));
    if (!created) return;
    recoveryPhrase = created.recoveryPhrase;
    status = created.status;
    password = '';
    confirmation = '';
    setupStep = 2;
  }
  async function completeSetup() {
    recoveryPhrase = '';
    await enterVault();
    changeScreen('library');
  }
  async function unlock() {
    const next = await run(() => invoke<Status>('vault_unlock', { password }));
    if (!next) return;
    status = next;
    password = '';
    await loadEntries();
    changeScreen('library');
  }
  async function unlockWithRecovery() {
    // Desde "Seguridad" la frase no desbloquea nada: se comprueba contra el
    // hueco de recuperación del Vault ya abierto.
    const verifying = screen === 'security' && !!status?.unlocked;
    const next = await run(() => invoke<Status>('vault_unlock_recovery', { phrase: restoreSecret }));
    if (!next) return;
    status = next;
    if (verifying) {
      finish('La frase de recuperación es válida para este Vault.');
      return;
    }
    await loadEntries();
    changeScreen('library');
    finish('Vault desbloqueado con la frase de recuperación.');
  }
  async function lockVault() {
    await run(() => invoke('vault_lock'));
    entries = [];
    selected = null;
    changeScreen('locked');
  }
  async function importEntry() {
    const entry = await run(() => invoke<Entry | null>('vault_import', { title: null }));
    if (!entry) {
      if (formError) return;
      finish('No se añadió ningún archivo.');
      return;
    }
    await reload();
    finish(`«${entry.title}» ya está cifrado en el Vault.`);
  }
  async function exportEntry(entry: Entry) {
    const destination = await run(() =>
      invoke<string | null>('vault_export_entry', { id: entry.id, defaultName: entry.title }),
    );
    if (!destination) return;
    finish(`Copia descifrada guardada en: ${destination}`);
  }
  async function deleteEntry() {
    if (!selected) return;
    await run(() => invoke('vault_delete_entry', { id: selected!.id }));
    if (formError) return;
    const title = selected.title;
    await reload();
    closeModal();
    notice = `«${title}» se ha eliminado del Vault. No hay papelera.`;
  }
  async function createBackup() {
    const destination = await run(() => invoke<string | null>('vault_backup'));
    if (!destination) return;
    finish(`Copia cifrada guardada en: ${destination}`);
  }
  async function restoreBackup() {
    const next = await run(() =>
      invoke<Status>('vault_restore', { secret: restoreSecret, replace: replaceExisting }),
    );
    if (!next) return;
    status = next;
    restoreSecret = '';
    await loadEntries();
    changeScreen('library');
    finish('Vault restaurado desde la copia.');
  }
  async function migrateTranscripts() {
    const report = await run(() => invoke<{ copied: number; remaining: number }>('vault_migrate_transcripts'));
    if (!report) return;
    await reload();
    finish(
      report.copied === 0
        ? 'No hay transcripciones pendientes de trasladar.'
        : `${report.copied} ${report.copied === 1 ? 'transcripción trasladada' : 'transcripciones trasladadas'} y verificadas. Quedan ${report.remaining} en el historial local.`,
    );
  }
</script>

<svelte:window onpointerdown={touch} onkeydown={touch} />

<section class="vault" aria-label="Vault">
  {#if !ready}
    <div class="boot"><span class="boot-dot"></span>Abriendo el Vault…</div>
  {:else if screen === 'locked' || screen === 'setup'}
    <div class="onboarding">
      <div class="intro-art" aria-hidden="true"><div class="orbit orbit-one"></div><div class="orbit orbit-two"></div><div class="door-plinth"><Icon name="door" size={128} /></div><span class="art-lock"><Icon name="lock" size={22} /></span></div>
      <div class="onboarding-copy">
        <span class="eyebrow">TU ESPACIO PRIVADO</span>
        {#if screen === 'locked'}
          <h1>Lo tuyo, bajo llave.</h1>
          <p>Tus documentos tienen su propio espacio.<br />Desbloquea el Vault para volver a ellos.</p>
          <form class="access-form" onsubmit={(event) => { event.preventDefault(); void unlock(); }}>
            <label for="vault-password">Contraseña del Vault</label>
            <input id="vault-password" type="password" autocomplete="off" bind:value={password} placeholder="Tu contraseña" />
            {#if formError}<p class="inline-error" role="alert">{formError}</p>{/if}
            <button class="v-primary" type="submit" disabled={busy}><Icon name="unlock" size={19} />{busy ? 'Abriendo…' : 'Desbloquear'}<Icon name="arrow" size={18} /></button>
          </form>
          <button class="text-button" onclick={(event) => openModal('recovery', event.currentTarget)}><Icon name="key" size={17} />Usar frase de recuperación</button>
          <div class="quiet-note"><Icon name="clock" size={17} /><span>El Vault se bloquea tras {(status?.autoLockSeconds ?? 300) / 60} min de inactividad.<br />La frase de recuperación es la única salida si olvidas la contraseña.</span></div>
        {:else}
          <div class="setup-progress" aria-label={`Paso ${setupStep} de 3`}>{#each ['Acceso', 'Recuperación', 'Listo'] as label, i}<span class:current={setupStep === i + 1} class:complete={setupStep > i + 1}><b>{setupStep > i + 1 ? '✓' : i + 1}</b>{label}</span>{/each}</div>
          {#if setupStep === 1}
            <h1>Una puerta solo tuya.</h1><p>Un Vault personal, en este equipo.<br />Empieza definiendo cómo entrarás.</p>
            <form class="access-form" onsubmit={(event) => { event.preventDefault(); void createVault(); }}>
              <label for="new-password">Contraseña del Vault</label><input id="new-password" type="password" autocomplete="new-password" bind:value={password} placeholder="8 caracteres o más" aria-describedby="setup-help" />
              <label for="repeat-password">Repetir contraseña</label><input id="repeat-password" type="password" autocomplete="new-password" bind:value={confirmation} placeholder="Repite la contraseña" aria-invalid={!!passwordError} aria-describedby="setup-help" />
              <span id="setup-help" class="form-help">Se usa para derivar la clave con Argon2id. No se guarda en ningún sitio.</span>
              {#if passwordError}<p class="inline-error" role="alert">{passwordError}</p>{/if}
              {#if formError}<p class="inline-error" role="alert">{formError}</p>{/if}
              <button class="v-primary" type="submit" disabled={busy}>{busy ? 'Creando…' : 'Continuar'}<Icon name="arrow" size={18} /></button>
            </form>
          {:else if setupStep === 2}
            <h1>Conserva tu acceso.</h1><p>La frase de recuperación te permitirá volver a entrar si olvidas la contraseña o cambias de equipo.</p>
            <div class="sample-label">TU FRASE · ANÓTALA FUERA DE ESTE EQUIPO</div>
            <div class="recovery-words">{#each words as word, i}<span><small>{String(i + 1).padStart(2, '0')}</small>{word}</span>{/each}</div>
            <div class="callout"><Icon name="warning" size={20} /><span>Sin contraseña ni frase no hay forma de abrir el Vault; nadie, tampoco la aplicación, puede recuperarlo. Anótala antes de continuar: no volverá a mostrarse.</span></div>
            <label class="checkbox-label"><input type="checkbox" bind:checked={recoverySaved} />He guardado la frase en un lugar privado y fuera de este equipo.</label>
            <button class="v-primary" disabled={!recoverySaved} onclick={() => (setupStep = 3)}>Continuar<Icon name="arrow" size={18} /></button>
          {:else}
            <h1>Todo en su lugar.</h1><p>Tu Vault está creado. Tú decides qué guardar; nada se incorpora automáticamente.</p>
            <div class="setup-summary"><span><Icon name="laptop" size={21} />Personal y local</span><span><Icon name="key" size={21} />Recuperación guardada</span><span><Icon name="clock" size={21} />Bloqueo automático · {(status?.autoLockSeconds ?? 300) / 60} min</span></div>
            <button class="v-primary" onclick={completeSetup}>Abrir mi Vault<Icon name="arrow" size={18} /></button>
          {/if}
        {/if}
      </div>
      <div class="onboarding-footer"><Icon name="laptop" size={17} />Un usuario. Un equipo. Tu espacio.</div>
    </div>
  {:else}
    <div class="vault-workspace">
      <aside class="vault-sidebar" aria-label="Navegación del Vault">
        <div class="vault-identity"><span class="vault-symbol"><Icon name="vault" size={28} /></span><div><strong>Mi Vault</strong><span>Espacio personal</span></div></div>
        <span class="sidebar-label">BIBLIOTECA</span>
        <nav>{#each categories as item}<button aria-label={item.label} title={item.label} class:active={screen === 'library' && category === item.id} aria-current={screen === 'library' && category === item.id ? 'page' : undefined} onclick={() => { changeScreen('library'); category = item.id; query = ''; }}><Icon name={item.icon} size={19} /><span>{item.label}</span><small>{count(item.id)}</small></button>{/each}</nav>
        <div class="sidebar-divider"></div>
        <nav><button aria-label="Seguridad y acceso" title="Seguridad y acceso" class:active={screen === 'security'} onclick={() => changeScreen('security')}><Icon name="shield" size={19} /><span>Seguridad y acceso</span></button><button aria-label="Copia de seguridad" title="Copia de seguridad" onclick={(event) => openModal('export', event.currentTarget)}><Icon name="download" size={19} /><span>Copia de seguridad</span></button></nav>
        <div class="sidebar-bottom"><div class="local-card"><Icon name="laptop" size={21} /><div><strong>Solo en este equipo</strong><span>Sin nube ni sincronización</span></div></div><button class="lock-action" aria-label="Bloquear Vault" title="Bloquear Vault" onclick={() => void lockVault()}><Icon name="lock" size={18} /><span>Bloquear Vault</span><Icon name="arrow" size={16} /></button></div>
      </aside>
      <main class="vault-main" bind:this={mainEl}>
        <header class="vault-heading"><div><span class="eyebrow">VAULT / {screen === 'security' ? 'PREFERENCIAS' : 'BIBLIOTECA'}</span><h1>{screen === 'security' ? 'Seguridad y acceso' : title}</h1><p>{screen === 'security' ? 'Tu acceso, tu recuperación y tus copias.' : 'Un lugar para lo que quieres conservar.'}</p></div><span class="status-pill"><Icon name="unlock" size={15} />Desbloqueado</span></header>
        {#if notice}<div class="notice" role="status"><Icon name="info" size={18} /><span>{notice}</span><button class="icon-button" title="Cerrar aviso" aria-label="Cerrar aviso" onclick={() => (notice = '')}><Icon name="close" size={16} /></button></div>{/if}
        {#if screen === 'security'}
          <div class="security-grid">
            <section class="security-card"><span class="feature-icon"><Icon name="lock" size={25} /></span><h2>Acceso al Vault</h2><p>Una contraseña para abrir tu espacio personal.</p><div class="setting-row"><span>Bloqueo por inactividad</span><strong>{(status?.autoLockSeconds ?? 300) / 60} minutos</strong></div><div class="setting-row"><span>Al suspender el equipo</span><strong>Pendiente</strong></div><div class="setting-row"><span>Cifrado</span><strong>XChaCha20-Poly1305</strong></div><div class="setting-row"><span>Derivación de clave</span><strong>Argon2id</strong></div></section>
            <section class="security-card"><span class="feature-icon"><Icon name="key" size={25} /></span><h2>Recuperación</h2><p>Una segunda forma de entrar si pierdes el acceso.</p><div class="callout compact"><Icon name="info" size={18} /><span>Conserva la frase fuera de este equipo. Sin ella, un disco averiado no se puede recuperar.</span></div><button class="v-secondary" onclick={(event) => openModal('recovery', event.currentTarget)}>Usar la frase ahora<Icon name="arrow" size={17} /></button></section>
            <section class="security-card"><span class="feature-icon"><Icon name="fingerprint" size={25} /></span><h2>Protección del equipo</h2><p>Estado real de este Mac.</p><div class="setting-row"><span>FileVault</span><strong>{status?.fileVault === null ? 'No comprobable' : status?.fileVault ? 'Activado' : 'Desactivado'}</strong></div><div class="setting-row"><span>Clave ligada al hardware</span><strong>{status?.hardwareProtection ? 'Sí' : 'Pendiente'}</strong></div>{#if status?.fileVault === false}<p class="small-copy warning-copy">Sin FileVault, otro sistema puede leer el disco. Actívalo en Ajustes del Sistema → Privacidad y seguridad.</p>{/if}<button class="text-button" onclick={(event) => openModal('hardware', event.currentTarget)}>Qué protege el Vault<Icon name="arrow" size={16} /></button></section>
            <section class="security-card"><span class="feature-icon"><Icon name="download" size={25} /></span><h2>Copia de seguridad</h2><p>Una copia cifrada para recuperar tus archivos si falla el disco.</p><div class="setting-row"><span>Entradas</span><strong>{status?.entryCount ?? 0}</strong></div><div class="setting-row"><span>Tamaño</span><strong>{formatBytes(status?.totalBytes ?? 0)}</strong></div><div class="button-row"><button class="v-primary" onclick={(event) => openModal('export', event.currentTarget)}><Icon name="download" size={17} />Crear copia</button><button class="icon-button outlined" title="Restaurar una copia" aria-label="Restaurar una copia" onclick={(event) => openModal('restore', event.currentTarget)}><Icon name="history" size={20} /></button></div></section>
          </div>
          <div class="migration-card"><Icon name="folder" size={25} /><div><h2>Trae tus transcripciones al Vault</h2><p>Se copian, se verifican y solo entonces se retiran del historial en claro.</p></div><button class="v-primary" onclick={(event) => openModal('migrate', event.currentTarget)} disabled={busy}>Trasladar</button></div>
          <p class="privacy-footnote"><Icon name="info" size={16} />Los títulos y el contenido están cifrados. El número de entradas y el tamaño total siguen siendo visibles en disco.</p>
        {:else}
          <div class="library-tools"><label class="search"><Icon name="search" size={19} /><input aria-label="Buscar en el Vault" bind:value={query} placeholder="Buscar en tu Vault…" />{#if query}<button class="icon-button" title="Limpiar búsqueda" aria-label="Limpiar búsqueda" onclick={() => (query = '')}><Icon name="close" size={16} /></button>{/if}</label><button class="v-primary" onclick={(event) => openModal('import', event.currentTarget)} title="Añadir al Vault"><Icon name="plus" size={20} />Añadir al Vault</button></div>
          <div class="collection-meta"><span>{visible.length} {visible.length === 1 ? 'entrada' : 'entradas'}<span class="meta-separator">/</span>{formatBytes(status?.totalBytes ?? 0)} cifrados</span><div class="collection-controls"><select aria-label="Ordenar entradas" bind:value={sort}><option value="recent">Más recientes</option><option value="name">Nombre A–Z</option></select><div class="view-switch" aria-label="Presentación"><button class:chosen={layout === 'grid'} class="icon-button" title="Vista de tarjetas" aria-label="Vista de tarjetas" aria-pressed={layout === 'grid'} onclick={() => (layout = 'grid')}><Icon name="grid" size={17} /></button><button class:chosen={layout === 'list'} class="icon-button" title="Vista de lista" aria-label="Vista de lista" aria-pressed={layout === 'list'} onclick={() => (layout = 'list')}><Icon name="list" size={19} /></button></div></div></div>
          <div class="library-body" class:has-selection={!!selected}>
            <div class="entry-area">
              {#if visible.length}
                <div class="entries" class:list-layout={layout === 'list'}>{#each visible as entry (entry.id)}<button class="entry-card" class:selected={selected?.id === entry.id} aria-pressed={selected?.id === entry.id} aria-label={`Ver detalle de ${entry.title}`} title={`Ver detalle de ${entry.title}`} onclick={() => (selected = selected?.id === entry.id ? null : entry)}><div class="entry-top"><span class="entry-icon"><Icon name={entry.kind} size={26} /></span><Icon name="chevron" size={16} /></div><div class="entry-copy"><span class="entry-type">{kindLabel(entry.kind)}</span><h2>{entry.title}</h2><span class="entry-date">{formatDate(entry.createdAt)}<span>·</span>{formatBytes(entry.size)}</span></div><div class="entry-footer"><span><Icon name="lock" size={13} />Cifrado</span><span class="entry-id">{entry.id.slice(0, 8)}</span></div></button>{/each}</div>
              {:else}
                <div class="empty-state"><span class="empty-icon"><Icon name={query ? 'search' : 'vault'} size={46} /></span><h2>{query ? 'No encontramos coincidencias' : 'Tu espacio empieza aquí'}</h2><p>{query ? 'Prueba con otro nombre o consulta toda la biblioteca.' : 'Guarda documentos, transcripciones y originales en un solo lugar. Tú eliges qué entra.'}</p><button class="v-primary" onclick={(event) => { if (query) { query = ''; category = 'all'; } else void openModal('import', event.currentTarget); }}><Icon name={query ? 'grid' : 'plus'} size={18} />{query ? 'Ver toda la biblioteca' : 'Añadir primera entrada'}</button></div>
              {/if}
            </div>
            {#if selected}
              <aside class="detail-panel" aria-label={`Detalle de ${selected.title}`}><div class="detail-header"><span class="eyebrow">DETALLE DE ENTRADA</span><button class="icon-button" title="Cerrar detalle" aria-label="Cerrar detalle" onclick={() => (selected = null)}><Icon name="close" size={18} /></button></div><span class="detail-icon"><Icon name={selected.kind} size={36} /></span><h2>{selected.title}</h2><p>{selected.description}</p><dl><div><dt>Tipo</dt><dd>{kindLabel(selected.kind)}</dd></div><div><dt>Añadido</dt><dd>{formatDate(selected.createdAt)}</dd></div><div><dt>Tamaño</dt><dd>{formatBytes(selected.size)}</dd></div><div><dt>Ubicación</dt><dd>Este equipo</dd></div><div><dt>Huella</dt><dd class="fingerprint">{selected.sha256.slice(0, 16)}…</dd></div></dl><div class="detail-note"><Icon name="shield" size={19} /><span>El contenido y el título están cifrados con una clave por entrada.</span></div><div class="detail-actions"><button class="v-secondary" onclick={() => void exportEntry(selected!)} disabled={busy}><Icon name="download" size={17} />Exportar descifrado</button><button class="text-button delete-button" onclick={(event) => openModal('delete', event.currentTarget)}><Icon name="trash" size={17} />Eliminar del Vault</button></div></aside>
            {/if}
          </div>
          <div class="library-bottom"><span><Icon name="shield" size={17} />XChaCha20-Poly1305 · Argon2id · sin salida a la red.</span></div>
        {/if}
      </main>
    </div>
  {/if}
</section>

<dialog bind:this={dialog} class="vault-dialog" aria-labelledby="vault-dialog-title" onclose={() => { password = ''; formError = ''; if (modalTrigger?.isConnected) modalTrigger.focus(); }}>
  <div class="modal-top"><span class="eyebrow">VAULT</span><button class="icon-button" title="Cerrar ventana" aria-label="Cerrar ventana" onclick={closeModal} disabled={busy}><Icon name="close" size={21} /></button></div>
  <span class="modal-icon"><Icon name={modal === 'delete' ? 'trash' : modal === 'recovery' ? 'key' : modal === 'hardware' ? 'fingerprint' : modal === 'import' ? 'plus' : modal === 'restore' ? 'history' : modal === 'migrate' ? 'folder' : 'download'} size={30} /></span>
  <h2 id="vault-dialog-title">{modal === 'import' ? 'Un lugar para tus archivos.' : modal === 'export' ? 'Una copia, por tranquilidad.' : modal === 'delete' ? '¿Eliminar esta entrada?' : modal === 'recovery' ? 'Recupera tu acceso.' : modal === 'migrate' ? 'Tus transcripciones, al Vault.' : modal === 'restore' ? 'Vuelve a tu Vault.' : 'Una llave en tu equipo.'}</h2>
  {#if modal === 'import'}
    <p>Elige el archivo que quieres conservar. Se cifra aquí mismo y el original permanece en su ubicación.</p><div class="mock-dropzone"><Icon name="upload" size={32} /><strong>Explorar archivos</strong><span>Documentos, audio y originales</span><small>Se guarda una copia cifrada; el original no se modifica</small></div><div class="callout"><Icon name="info" size={19} /><span>Importar al Vault no implica anonimizar el contenido.</span></div>
  {:else if modal === 'export'}
    <p>La copia contiene todo el Vault cifrado: originales, transcripciones y claves envueltas. Se abre con tu contraseña o con tu frase de recuperación.</p><div class="export-file"><Icon name="vault" size={28} /><div><strong>Mi Vault · Copia cifrada</strong><span>{status?.entryCount ?? 0} entradas · {formatBytes(status?.totalBytes ?? 0)}</span></div></div><div class="callout"><Icon name="key" size={21} /><span>Conserva también tu frase de recuperación fuera del equipo. La frase no sustituye a la copia si el disco falla.</span></div>
  {:else if modal === 'delete'}
    <p><strong>{selected?.title}</strong></p><p>Eliminar una entrada es permanente: se destruye la clave que la protege y no hay papelera. Las copias exportadas antes no se eliminan.</p><div class="callout"><Icon name="warning" size={20} /><span>El contenido será irrecuperable, también con la frase de recuperación.</span></div>
  {:else if modal === 'recovery'}
    <p>Escribe las 12 palabras de tu frase de recuperación, separadas por espacios.</p><label class="field-label" for="recovery-input">Frase de recuperación</label><textarea id="recovery-input" rows="3" bind:value={restoreSecret} placeholder="palabra palabra palabra…" aria-describedby="recovery-help"></textarea><span class="form-help" id="recovery-help">Se comprueba en este equipo; no se envía a ningún sitio.</span>
  {:else if modal === 'migrate'}
    <p>Se copiarán las transcripciones del historial local, se verificarán una a una y solo entonces se retirarán del historial en claro.</p><ol class="migration-steps"><li><span>01</span>Cifrar cada transcripción</li><li><span>02</span>Descifrar y comparar la huella</li><li><span>03</span>Retirar del historial local</li></ol><div class="callout"><Icon name="info" size={19} /><span>Si la verificación falla, el historial queda intacto.</span></div>
  {:else if modal === 'restore'}
    <p>Elige una copia cifrada y escribe la contraseña o la frase con la que se creó. Se verifica antes de escribir nada.</p><label class="field-label" for="restore-secret">Contraseña o frase de recuperación</label><textarea id="restore-secret" rows="2" bind:value={restoreSecret} placeholder="Contraseña o 12 palabras"></textarea>{#if status?.initialized}<label class="checkbox-label"><input type="checkbox" bind:checked={replaceExisting} />Sustituir el Vault actual de este equipo</label>{/if}<div class="callout"><Icon name="info" size={19} /><span>Si la copia no se puede abrir, el Vault actual no se toca.</span></div>
  {:else}
    <p>El contenido se cifra en reposo con XChaCha20-Poly1305 y una clave por entrada, envuelta por la clave maestra. La clave maestra se guarda envuelta con Argon2id a partir de tu contraseña y de tu frase de recuperación.</p><div class="callout"><Icon name="info" size={20} /><span>Pendiente: ligar la clave al Secure Enclave (macOS) o al TPM (Windows) para que otro sistema operativo no pueda usarla aunque copie el disco. Hoy el Vault depende de FileVault y de la contraseña.</span></div>
  {/if}
  {#if formError}<p class="inline-error modal-error" role="alert">{formError}</p>{/if}
  <div class="modal-footer"><button class="v-secondary" onclick={closeModal} disabled={busy}>{modal === 'hardware' ? 'Cerrar' : 'Cancelar'}</button>{#if modal !== 'hardware'}<button class="v-primary" onclick={() => {
    if (modal === 'delete') void deleteEntry();
    else if (modal === 'import') void importEntry();
    else if (modal === 'export') void createBackup();
    else if (modal === 'recovery') void unlockWithRecovery();
    else if (modal === 'restore') void restoreBackup();
    else if (modal === 'migrate') void migrateTranscripts();
  }} disabled={busy || (modal === 'restore' && status?.initialized && !replaceExisting)}>
    {busy ? 'Procesando…' : modal === 'delete' ? 'Eliminar definitivamente' : modal === 'import' ? 'Elegir archivo' : modal === 'export' ? 'Guardar copia' : modal === 'recovery' ? (screen === 'security' ? 'Comprobar' : 'Desbloquear') : modal === 'restore' ? 'Restaurar' : 'Trasladar'}<Icon name="arrow" size={17} /></button>{/if}</div>
  <span class="modal-disclaimer">{modal === 'hardware' ? 'Cifrado en reposo con claves derivadas de tu contraseña.' : 'Nada sale de este equipo.'}</span>
</dialog>

<style>
  .vault { flex:1; min-height:0; display:flex; flex-direction:column; color:var(--text); }
  .vault button,.vault-dialog button { cursor:pointer; font:inherit; }
  .vault button:disabled,.vault-dialog button:disabled { opacity:.45; cursor:default; }
  .vault button:focus-visible,.vault input:focus-visible,.vault select:focus-visible { outline:2px solid #fff; outline-offset:3px; }
  .boot { flex:1; display:flex; align-items:center; justify-content:center; gap:10px; color:var(--muted); font-size:12px; }
  .boot-dot { width:6px; height:6px; border-radius:50%; background:#bfd8ff; }
  .vault-workspace { flex:1; display:flex; min-height:0; }
  .vault-sidebar { flex:0 0 232px; min-height:0; overflow-y:auto; padding:30px 18px 20px; display:flex; flex-direction:column; border-right:1px solid var(--line); }
  .vault-identity { display:flex; gap:13px; align-items:center; margin:0 12px 35px; }
  .vault-symbol { width:46px; height:48px; border:1px solid #bfd8ff66; border-radius:12px; display:grid; place-items:center; color:#c5dbff; }
  .vault-identity strong { display:block; font-size:17px; font-weight:600; letter-spacing:-.3px; }
  .vault-identity div>span { display:block; color:var(--muted); font-size:11px; margin-top:3px; }
  .sidebar-label { font-size:9px; letter-spacing:1.6px; color:var(--muted); padding:0 13px 13px; }
  nav { display:flex; flex-direction:column; gap:6px; }
  nav button { border:1px solid transparent; color:var(--muted); background:transparent; display:flex; align-items:center; text-align:left; gap:12px; padding:12px; border-radius:8px; font-size:12px!important; min-height:44px; }
  nav button:hover { background:#ffffff09; color:#fff; }
  nav button.active { background:#bfd8ff18; border-color:#bfd8ff35; color:#fff; }
  nav button small { margin-left:auto; font-size:10px; opacity:.7; }
  nav button :global(svg) { flex-shrink:0; }
  .sidebar-divider { height:1px; background:var(--line); margin:22px 12px; }
  .sidebar-bottom { margin-top:auto; padding-top:32px; }
  .local-card { display:flex; gap:11px; padding:16px 10px; align-items:center; color:var(--muted); }
  .local-card strong { font-size:11px; font-weight:500; display:block; }
  .local-card div>span { display:block; font-size:10px; margin-top:3px; opacity:.8; }
  .lock-action { color:white; display:flex; align-items:center; gap:11px; border:1px solid var(--line); width:100%; padding:12px; border-radius:8px; background:transparent; font-size:12px!important; }
  .lock-action :global(svg:last-child) { margin-left:auto; }
  .vault-main { flex:1; min-width:0; padding:26px 36px 18px; overflow:auto; }
  .vault-heading { display:flex; align-items:center; justify-content:space-between; gap:16px; margin-bottom:24px; }
  .eyebrow { color:var(--muted); font-size:9px; letter-spacing:1.7px; font-weight:500; }
  h1 { font-size:32px; line-height:1.15; font-weight:500; letter-spacing:-1px; margin:10px 0 10px; }
  .vault-heading p { margin:0; color:var(--muted); font-size:12px; }
  .status-pill { font-size:10px; color:#d4e4ff; border:1px solid var(--line); border-radius:30px; padding:7px 11px; display:flex; align-items:center; gap:7px; white-space:nowrap; }
  .library-tools { display:flex; gap:16px; align-items:center; }
  .search { flex:1; display:flex; align-items:center; gap:11px; min-height:44px; border:1px solid var(--line); border-radius:8px; padding:0 14px; color:var(--muted); }
  .vault .search input { border:0; background:transparent; outline:none; min-width:0; width:100%; padding:11px 0; font-size:12px; }
  .search:focus-within { outline:2px solid #fff; outline-offset:3px; }
  input::placeholder,textarea::placeholder { color:var(--muted); opacity:1; }
  .v-primary,.v-secondary { display:inline-flex; align-items:center; justify-content:center; gap:10px; border-radius:8px; padding:11px 17px; min-height:44px; font-size:12px!important; font-weight:600!important; text-align:center; }
  .v-primary { background:#fff; color:#1248bc; border:1px solid #fff; }
  .v-primary:hover { background:#eaf2ff; }
  .v-secondary { color:#fff; background:transparent; border:1px solid var(--line-dark); }
  .v-secondary:hover,.icon-button:hover,.lock-action:hover { background:#bfd8ff18; }
  .collection-meta { display:flex; align-items:center; justify-content:space-between; gap:12px; margin:20px 0 14px; font-size:10px; color:var(--muted); }
  .meta-separator { margin:0 10px; opacity:.5; }
  .collection-controls { display:flex; align-items:center; gap:14px; }
  .collection-controls select { font-size:10px; border:0!important; background:transparent!important; padding:6px; }
  .view-switch { display:flex; border:1px solid var(--line); border-radius:6px; padding:2px; gap:3px; }
  .icon-button { width:30px; height:30px; padding:0; display:inline-flex; align-items:center; justify-content:center; border:0; border-radius:5px; background:transparent; color:var(--muted); flex-shrink:0; }
  .icon-button.chosen { background:#bfd8ff26; color:#fff; }
  .library-body { display:flex; gap:22px; align-items:stretch; }
  .entry-area { flex:1; min-width:0; }
  .entries { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:16px; }
  .entry-card { min-width:0; min-height:180px; display:flex; flex-direction:column; padding:20px 20px 0; border:1px solid #bfd8ff45; border-radius:12px; color:#fff; background:linear-gradient(130deg,#bfd8ff0e,transparent); text-align:left; transition:background .15s,border-color .15s; }
  .entry-card:hover,.entry-card.selected { background:#bfd8ff18; border-color:#d2e4ffb3; }
  .entry-top { display:flex; align-items:center; justify-content:space-between; color:#bfd8ffb3; margin-bottom:17px; }
  .entry-icon { display:flex; color:#bfd8ff; opacity:.8; }
  .entry-top>:global(svg) { opacity:0; }
  .entry-card:hover .entry-top>:global(svg),.entry-card.selected .entry-top>:global(svg) { opacity:1; }
  .entry-copy { min-width:0; }
  .entry-type { color:var(--muted); font-size:9px; }
  .entry-card h2 { font-size:15px; line-height:1.4; font-weight:500; margin:4px 0 8px; overflow-wrap:anywhere; letter-spacing:-.2px; }
  .entry-date { display:flex; align-items:center; gap:8px; color:var(--muted); font-size:11px; }
  .entry-footer { margin-top:18px; border-top:1px solid var(--line); padding:11px 0; display:flex; justify-content:space-between; gap:8px; width:100%; color:#c5dbff99; font-size:9px; }
  .entry-footer>span:first-child { display:flex; align-items:center; gap:5px; }
  .entry-id { font-variant-numeric:tabular-nums; }
  .has-selection .entries { grid-template-columns:repeat(2,minmax(0,1fr)); }
  .detail-panel { width:265px; flex-shrink:0; border:1px solid var(--line); border-radius:12px; background:#082f8526; padding:16px 20px; }
  .detail-header { display:flex; align-items:center; justify-content:space-between; gap:8px; }
  .detail-header .eyebrow { font-size:8px; }
  .detail-icon { display:flex; color:var(--muted); margin:28px 0 20px; }
  .detail-panel h2 { font-size:20px; font-weight:500; line-height:1.3; letter-spacing:-.5px; margin:0 0 12px; overflow-wrap:anywhere; }
  .detail-panel p { font-size:11px; color:var(--muted); line-height:1.7; }
  dl { margin:24px 0; font-size:10px; }
  dl>div { display:flex; justify-content:space-between; gap:10px; margin:13px 0; }
  dt { color:var(--muted); } dd { margin:0; text-align:right; }
  .fingerprint { font-family:ui-monospace,SFMono-Regular,Menlo,monospace; font-size:9px; letter-spacing:.2px; }
  .detail-note { display:flex; gap:9px; color:var(--muted); font-size:10px; line-height:1.6; padding-top:17px; border-top:1px solid var(--line); }
  .detail-note :global(svg) { flex-shrink:0; }
  .detail-actions { margin-top:26px; display:flex; flex-direction:column; gap:12px; }
  .detail-actions .v-secondary { font-size:11px!important; }
  .text-button { display:inline-flex; align-items:center; justify-content:center; gap:8px; padding:5px 0; background:transparent; border:0; color:var(--muted); font-size:11px!important; }
  .text-button:hover { color:#fff; }
  .delete-button { font-size:10px!important; color:#ffd7a3; }
  .list-layout.entries,.has-selection .list-layout.entries { grid-template-columns:1fr; gap:8px; }
  .list-layout .entry-card { display:flex; flex-direction:row; align-items:center; min-height:83px; padding:14px 18px; gap:20px; }
  .list-layout .entry-top { margin:0; }.list-layout .entry-top>:global(svg) { display:none; }
  .list-layout .entry-copy { flex:1; }.list-layout .entry-card h2 { margin:2px 0 5px; }
  .list-layout .entry-footer { width:auto; border:0; margin:0; }.list-layout .entry-id { display:none; }
  .library-bottom { display:flex; justify-content:space-between; gap:16px; align-items:center; margin-top:28px; border-top:1px solid var(--line); padding-top:16px; font-size:10px; color:var(--muted); }
  .library-bottom>span { display:flex; align-items:center; gap:8px; }
  .empty-state { min-height:370px; display:flex; flex-direction:column; align-items:center; justify-content:center; padding:30px; text-align:center; border:1px dashed #bfd8ff66; border-radius:12px; }
  .empty-icon { color:var(--muted); margin-bottom:20px; }
  .empty-state h2 { font-size:24px; font-weight:500; letter-spacing:-.6px; margin:0; }
  .empty-state p { color:var(--muted); font-size:12px; max-width:320px; margin:12px 0 24px; }
  .notice { display:flex; align-items:center; gap:10px; padding:10px 14px; font-size:11px; background:#bfd8ff15; border:1px solid var(--line); border-radius:8px; margin-bottom:20px; }
  .notice .icon-button { margin-left:auto; }
  .security-grid { display:grid; grid-template-columns:1fr 1fr; gap:18px; }
  .security-card { display:flex; flex-direction:column; align-items:flex-start; border:1px solid var(--line); border-radius:12px; padding:22px; background:linear-gradient(130deg,#bfd8ff0c,transparent); }
  .feature-icon { display:flex; color:var(--muted); margin-bottom:18px; }
  .security-card h2,.migration-card h2 { font-size:16px; font-weight:500; margin:0 0 8px; letter-spacing:-.3px; }
  .security-card p { color:var(--muted); font-size:11px; margin:0 0 18px; }
  .setting-row { display:flex; width:100%; justify-content:space-between; align-items:center; gap:12px; font-size:10px; padding:10px 0; border-top:1px solid var(--line); color:var(--muted); }
  .setting-row strong { font-weight:500; color:#fff; }
  .security-card>.v-secondary,.security-card>.text-button,.security-card>.button-row { margin-top:auto; }
  .security-card>.v-secondary { width:100%; margin-top:18px; }
  .button-row { display:flex; width:100%; align-items:center; gap:12px; padding-top:18px; }.button-row .v-primary { flex:1; }
  .outlined { border:1px solid var(--line); width:44px; height:44px; border-radius:8px; }
  .small-copy { font-size:10px; color:var(--muted); line-height:1.6; }
  .security-card .small-copy { font-size:10px; }
  .warning-copy { color:#ffd7a3; margin-top:12px!important; }
  .migration-card { display:flex; align-items:center; gap:18px; margin-top:18px; padding:22px; border:1px solid var(--line); border-radius:12px; }
  .migration-card>:global(svg) { color:var(--muted); flex-shrink:0; }.migration-card>div { flex:1; }
  .migration-card h2 { font-size:13px; }.migration-card p { color:var(--muted); font-size:10px; margin:0; }
  .migration-card .v-primary { flex-shrink:0; }
  .privacy-footnote { display:flex; align-items:flex-start; gap:8px; color:var(--muted); font-size:10px; margin:20px 0 0; }.privacy-footnote :global(svg) { flex-shrink:0; }
  .onboarding { flex:1; min-height:0; overflow:auto; position:relative; display:grid; grid-template-columns:1fr 1fr; align-items:center; gap:36px; padding:48px max(48px,7vw) 70px; }
  .onboarding-copy { max-width:430px; width:100%; justify-self:center; }.onboarding h1 { font-size:38px; margin:14px 0 18px; letter-spacing:-1.3px; }.onboarding-copy>p { font-size:13px; line-height:1.8; color:var(--muted); margin:0 0 26px; }
  .intro-art { width:330px; height:360px; position:relative; display:grid; place-items:center; justify-self:center; }
  .orbit { position:absolute; border:1px solid #bfd8ff21; border-radius:50%; width:330px; height:330px; }.orbit-two { width:246px; height:246px; border-color:#bfd8ff33; }
  .door-plinth { width:170px; height:196px; border:1px solid #bfd8ff55; border-radius:32px; background:linear-gradient(145deg,#bfd8ff17,#bfd8ff03); display:grid; place-items:center; color:#bfd8ffcc; box-shadow:0 24px 80px #052e6e25; }
  .art-lock { position:absolute; display:grid; place-items:center; width:52px; height:52px; border:1px solid #bfd8ff66; border-radius:16px; background:#1c52c2; bottom:67px; right:66px; color:#fff; }
  .access-form { display:flex; flex-direction:column; gap:10px; }.access-form label,.field-label { font-size:11px; }.access-form input { min-height:46px; padding:12px 14px; font-size:12px; width:100%; }.access-form .v-primary { margin-top:8px; }.access-form+.text-button { width:100%; margin-top:18px; }
  .quiet-note { margin-top:32px; padding-top:22px; border-top:1px solid var(--line); display:flex; gap:11px; color:var(--muted); font-size:10px; line-height:1.7; }.quiet-note :global(svg) { flex-shrink:0; margin-top:3px; }
  .onboarding-footer { grid-column:1/-1; display:flex; justify-content:center; align-items:center; gap:9px; color:var(--muted); font-size:10px; }
  .setup-progress { display:flex; gap:22px; margin:18px 0 28px; }.setup-progress>span { display:flex; align-items:center; gap:7px; font-size:10px; color:var(--muted); }.setup-progress b { font-size:9px; font-weight:500; width:20px; height:20px; border:1px solid var(--line); border-radius:50%; display:grid; place-items:center; }.setup-progress .current { color:white; }.current b,.complete b { background:white; color:#1248bc; }
  .form-help { font-size:10px; color:var(--muted); display:block; line-height:1.6; }.inline-error { margin:0; padding:10px; border:1px solid #bfd8ff99; border-radius:6px; font-size:11px; background:#082f8540; }
  .sample-label { color:var(--muted); font-size:8px; letter-spacing:1.1px; margin-bottom:11px; }.recovery-words { display:grid; grid-template-columns:repeat(3,1fr); gap:8px; margin-bottom:18px; }.recovery-words>span { display:flex; gap:10px; align-items:center; padding:11px; border:1px solid var(--line); border-radius:6px; font-size:12px; }.recovery-words small { font-size:8px; color:var(--muted); }
  .checkbox-label { display:flex; gap:10px; align-items:flex-start; font-size:11px; color:var(--muted); margin:20px 0; }.checkbox-label input { accent-color:white; margin:3px 0 0; }.onboarding-copy>.v-primary { width:100%; }
  .setup-summary { display:flex; flex-direction:column; gap:20px; margin:30px 0; padding:24px 0; border-block:1px solid var(--line); }.setup-summary>span { display:flex; align-items:center; gap:12px; color:var(--muted); font-size:12px; }
  .callout { display:flex; align-items:flex-start; gap:11px; border:1px solid var(--line); border-radius:8px; padding:14px; font-size:11px; color:var(--muted); line-height:1.7; margin:18px 0; }.callout :global(svg) { flex-shrink:0; margin-top:2px; }.callout.compact { font-size:10px; margin:0 0 16px; }
  .vault-dialog { width:min(500px,calc(100vw - 40px)); max-height:calc(100dvh - 48px); overflow:auto; color:#fff; background:#1248bc; padding:26px 30px; border:1px solid #bfd8ff80; border-radius:18px; box-shadow:0 32px 100px #001e6266; }.vault-dialog::backdrop { background:#031d54a6; backdrop-filter:blur(6px); }
  .modal-top { display:flex; align-items:center; justify-content:space-between; }.modal-icon { display:flex; color:var(--muted); margin:24px 0; }.vault-dialog h2 { font-size:28px; font-weight:500; letter-spacing:-.8px; line-height:1.2; margin:0 0 18px; }.vault-dialog p { font-size:12px; color:var(--muted); line-height:1.8; }.vault-dialog p strong { color:white; font-weight:500; }
  .mock-dropzone { padding:26px 18px; border:1px dashed #bfd8ff80; border-radius:10px; display:flex; flex-direction:column; align-items:center; gap:10px; margin:24px 0; color:var(--muted); text-align:center; }.mock-dropzone strong { font-size:13px; font-weight:500; color:#fff; }.mock-dropzone span { font-size:11px; }.mock-dropzone small { font-size:9px; }
  .export-file { padding:20px 16px; background:#bfd8ff0c; border:1px solid var(--line); border-radius:8px; display:flex; gap:15px; align-items:center; margin:22px 0; }.export-file strong { display:block; font-size:12px; font-weight:500; }.export-file span { display:block; font-size:10px; color:var(--muted); margin-top:5px; }
  .modal-footer { display:flex; justify-content:flex-end; gap:12px; padding-top:24px; margin-top:24px; border-top:1px solid var(--line); }.modal-disclaimer { display:block; text-align:center; color:var(--muted); font-size:9px; margin:18px 0 0; }.vault-dialog .field-label { display:block; margin:20px 0 8px; }.vault-dialog textarea { display:block; width:100%; padding:12px; font:inherit; font-size:12px; background:var(--panel); border:1px solid var(--line); border-radius:8px; resize:none; color:#fff; margin-bottom:8px; }
  .modal-error { margin:16px 0 0; }
  .migration-steps { padding:0; list-style:none; margin:24px 0; }.migration-steps li { display:flex; align-items:center; gap:14px; padding:14px 0; border-bottom:1px solid var(--line); font-size:12px; }.migration-steps li span { color:var(--muted); font-size:10px; }
  .vault-dialog button:focus-visible,.vault-dialog textarea:focus-visible { outline:2px solid white; outline-offset:4px; }
  @media(min-width:1550px) { .vault-main { padding-inline:48px; }.entry-card { min-height:230px; padding:24px 24px 0; }.entry-card h2 { font-size:16px; }.entry-top { margin-bottom:30px; }.vault-heading { margin-bottom:36px; } }
  @media(max-width:1150px) { .vault-sidebar { flex-basis:205px; padding-inline:12px; }.vault-main { padding:26px 24px 18px; }.entry-card { padding:17px 16px 0; }.entries { gap:12px; }.has-selection .entries { grid-template-columns:1fr; }.detail-panel { width:250px; }.vault-heading h1 { font-size:29px; }.status-pill { font-size:9px; }.onboarding { padding-inline:40px; gap:28px; }.intro-art { transform:scale(.85); }.onboarding h1 { font-size:32px; } }
  @media(max-width:900px) { .entries { grid-template-columns:repeat(2,minmax(0,1fr)); }.status-pill { display:none; }.security-grid { grid-template-columns:1fr; }.onboarding { grid-template-columns:1fr; padding:32px 24px; gap:24px; }.intro-art { display:none; }.onboarding-footer { margin-top:12px; }.onboarding-copy { max-width:440px; } }
  @media(max-width:650px) { .vault-sidebar { flex-basis:64px; padding:22px 8px; }.vault-identity { margin:0 0 24px; }.vault-identity div,.sidebar-label,nav button span,nav button small,.local-card,.lock-action span,.lock-action :global(svg:last-child) { display:none; }.vault-symbol { width:46px; height:46px; }nav button { justify-content:center; padding:12px 8px; }.sidebar-divider { margin:18px 4px; }.lock-action { justify-content:center; padding:12px 8px; }.vault-main { padding:22px 18px; }.library-tools { flex-wrap:wrap; }.search { flex-basis:100%; }.entries { grid-template-columns:1fr; }.library-body { flex-direction:column; }.detail-panel { width:100%; }.library-bottom { align-items:flex-start; }.library-bottom>span { display:none; }.collection-meta>span { font-size:9px; }.meta-separator { margin:0 4px; }.collection-controls { gap:6px; }.vault-heading h1 { font-size:26px; }.list-layout .entry-footer { display:none; }.migration-card { padding:16px; }.security-card { padding:18px; } }
  @media(max-height:740px) {
    .vault-sidebar { padding:20px 12px 14px; }
    .vault-identity { margin:0 10px 20px; }
    .sidebar-label { padding-bottom:10px; }
    nav { gap:4px; }
    nav button { padding:9px 12px; min-height:38px; }
    .sidebar-divider { margin:16px 12px; }
    .sidebar-bottom { padding-top:16px; }
    .local-card { display:none; }
  }
  @media(prefers-reduced-motion:reduce) { .entry-card { transition:none; } }
</style>
