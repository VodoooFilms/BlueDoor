import { browserType } from './browser.mjs';
import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';

// Ejercita la interfaz real del Vault contra un límite IPC determinista: el
// backend nativo tiene sus propias pruebas en `src-tauri/src/vault/tests.rs`.
mkdirSync('artifacts/vault', { recursive: true });
const browser = await browserType.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 828 }, reducedMotion: 'reduce' });
  page.setDefaultTimeout(10000);
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    localStorage.setItem('bluedoor-welcome-seen', 'true');
    localStorage.setItem('bluedoor-language', 'es');
    const phrase = 'puerta brisa cobre luna bosque tinta isla nube faro papel rio azul';
    const vault = {
      initialized: false,
      unlocked: false,
      password: 'frase-de-paso-larga',
      phrase,
      entries: [],
      imports: 0,
      backups: 0,
      restores: 0,
    };
    window.__vault = vault;
    window.__commands = [];
    window.__exports = [];
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    const entry = (id, title, kind, size, createdAt) => ({
      id,
      title,
      kind,
      mime: kind === 'audio' ? 'application/json' : 'application/pdf',
      size,
      createdAt,
      description:
        kind === 'audio'
          ? 'Transcripción y audio guardados en este equipo.'
          : 'Documento guardado en este equipo, cifrado en reposo.',
      sha256: '9f2c1d4e5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d',
    });
    const status = () => ({
      initialized: vault.initialized,
      unlocked: vault.unlocked,
      entryCount: vault.unlocked ? vault.entries.length : 0,
      totalBytes: vault.unlocked ? vault.entries.reduce((sum, item) => sum + item.size, 0) : 0,
      autoLockSeconds: 300,
      createdAt: vault.initialized ? '2026-10-23T09:00:00Z' : null,
      fileVault: true,
      hardwareProtection: false,
    });
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
      transformCallback: () => 1,
      invoke: async (cmd, args) => {
        window.__commands.push(cmd);
        if (cmd === 'plugin:event|listen') return 1;
        if (cmd === 'load_transcripts') return [];
        if (cmd === 'get_privacy_session') return null;
        if (cmd === 'list_models') {
          return { whisper: [{ installed: true }], parakeet: [], privacy: [{ id: 'qwen3-fast', installed: true }] };
        }
        if (['check_installation', 'save_transcripts', 'plugin:event|unlisten'].includes(cmd)) return;
        switch (cmd) {
          case 'vault_status':
            return status();
          case 'vault_create': {
            if (vault.initialized) throw 'Ya existe un Vault en este equipo.';
            if (!args?.password || args.password.length < 8) throw 'La contraseña debe tener al menos 8 caracteres.';
            vault.initialized = true;
            vault.unlocked = true;
            return { recoveryPhrase: vault.phrase, status: status() };
          }
          case 'vault_unlock':
            if (args?.password !== vault.password) throw 'La contraseña no es correcta.';
            vault.unlocked = true;
            return status();
          case 'vault_unlock_recovery':
            if (args?.phrase !== vault.phrase) throw 'La frase de recuperación no es válida.';
            vault.unlocked = true;
            return status();
          case 'vault_lock':
            vault.unlocked = false;
            return null;
          case 'vault_list':
            if (!vault.unlocked) throw 'El Vault está bloqueado.';
            return vault.entries.slice();
          case 'vault_import': {
            if (!vault.unlocked) throw 'El Vault está bloqueado.';
            const first = vault.imports++ === 0;
            const item = first
              ? entry('11111111-2222-4333-8444-555555555555', 'Contrato de servicios.pdf', 'document', 248000, '2026-10-23T10:00:00Z')
              : entry('66666666-7777-4888-8999-aaaaaaaaaaaa', 'Entrevista inicial.wav', 'audio', 18400, '2026-10-23T11:00:00Z');
            vault.entries.push(args?.title ? { ...item, title: args.title } : item);
            return vault.entries[vault.entries.length - 1];
          }
          case 'vault_export_entry':
            window.__exports.push({ id: args?.id, defaultName: args?.defaultName });
            return `/tmp/${args?.defaultName}`;
          case 'vault_delete_entry': {
            const index = vault.entries.findIndex(item => item.id === args?.id);
            if (index < 0) throw 'La entrada ya no existe.';
            vault.entries.splice(index, 1);
            return null;
          }
          case 'vault_backup':
            vault.backups += 1;
            return '/tmp/Mi Vault.bluedoor';
          case 'vault_restore': {
            if (vault.initialized && !args?.replace) throw 'Ya existe un Vault en este equipo.';
            if (args?.secret !== vault.password && args?.secret !== vault.phrase) {
              throw 'La contraseña o la frase no son válidas.';
            }
            vault.restores += 1;
            vault.initialized = true;
            vault.unlocked = true;
            vault.entries = [
              entry('11111111-2222-4333-8444-555555555555', 'Contrato de servicios.pdf', 'document', 248000, '2026-10-23T10:00:00Z'),
              entry('66666666-7777-4888-8999-aaaaaaaaaaaa', 'Entrevista inicial.wav', 'audio', 18400, '2026-10-23T11:00:00Z'),
            ];
            return status();
          }
          case 'vault_migrate_transcripts':
            vault.entries.push(
              entry('bbbbbbbb-cccc-4ddd-8eee-ffffffffffff', 'Vista oral', 'audio', 32000, '2026-10-20T10:00:00Z'),
              entry('cccccccc-dddd-4eee-8fff-000000000000', 'Reunión de seguimiento', 'audio', 18000, '2026-10-19T10:00:00Z'),
            );
            return { copied: 2, remaining: 0 };
          default:
            throw Error('Unexpected command: ' + cmd);
        }
      },
    };
  });

  await page.goto('http://localhost:1420');
  await page.locator('.blue-import-screen').waitFor();
  const storage = await page.evaluate(() => JSON.stringify(localStorage));
  await page.getByRole('button', { name: 'Vault', exact: true }).click();
  const vault = page.locator('.vault');
  const dialog = page.getByRole('dialog');

  // 1. Primer arranque: crear el Vault.
  await vault.getByRole('heading', { name: 'Una puerta solo tuya.' }).waitFor();
  await vault.getByRole('button', { name: 'Continuar', exact: true }).click();
  await vault.getByRole('alert').filter({ hasText: 'Usa al menos 8 caracteres.' }).waitFor();
  await vault.getByLabel('Contraseña del Vault', { exact: true }).fill('frase-de-paso-larga');
  await vault.getByLabel('Repetir contraseña', { exact: true }).fill('otra-distinta');
  await vault.getByRole('button', { name: 'Continuar', exact: true }).click();
  await vault.getByRole('alert').filter({ hasText: 'Las dos contraseñas no coinciden.' }).waitFor();
  await vault.getByLabel('Repetir contraseña', { exact: true }).fill('frase-de-paso-larga');
  await vault.getByRole('button', { name: 'Continuar', exact: true }).click();

  // 2. La frase de recuperación se muestra una sola vez.
  await vault.getByRole('heading', { name: 'Conserva tu acceso.' }).waitFor();
  assert.equal(await vault.locator('.recovery-words > span').count(), 12, 'the recovery phrase has 12 words');
  const phrase = await page.evaluate(() => window.__vault.phrase);
  assert(await vault.getByRole('button', { name: 'Continuar', exact: true }).isDisabled(), 'cannot continue before saving the phrase');
  await page.screenshot({ path: 'artifacts/vault/01-recovery.png' });
  await vault.getByRole('checkbox').check();
  await vault.getByRole('button', { name: 'Continuar', exact: true }).click();
  await vault.getByRole('heading', { name: 'Todo en su lugar.' }).waitFor();
  await vault.getByRole('button', { name: 'Abrir mi Vault' }).click();
  await vault.getByRole('heading', { name: 'Tu espacio empieza aquí' }).waitFor();
  assert.equal(await vault.locator('.entry-card').count(), 0, 'a new Vault starts empty');
  assert(!(await page.evaluate(() => document.body.innerText)).includes(phrase), 'the phrase is not left on screen');
  await page.screenshot({ path: 'artifacts/vault/02-empty.png' });

  // 3. Importar dos archivos reales (el diálogo nativo lo simula el IPC).
  await vault.getByRole('button', { name: 'Añadir primera entrada' }).click();
  await dialog.waitFor();
  await page.screenshot({ path: 'artifacts/vault/03-import.png' });
  await dialog.getByRole('button', { name: 'Elegir archivo' }).click();
  await vault.getByText('«Contrato de servicios.pdf» ya está cifrado en el Vault.').waitFor();
  assert.equal(await vault.locator('.entry-card').count(), 1);
  await vault.getByRole('button', { name: 'Añadir al Vault', exact: true }).click();
  await dialog.getByRole('button', { name: 'Elegir archivo' }).click();
  await vault.getByText('«Entrevista inicial.wav» ya está cifrado en el Vault.').waitFor();
  assert.equal(await vault.locator('.entry-card').count(), 2);
  await page.screenshot({ path: 'artifacts/vault/04-library.png' });

  // 4. Búsqueda, filtros y orden.
  await vault.getByRole('textbox', { name: 'Buscar en el Vault' }).fill('contrato');
  assert.equal(await vault.locator('.entry-card').count(), 1);
  await vault.getByRole('textbox', { name: 'Buscar en el Vault' }).fill('no existe');
  await vault.getByRole('heading', { name: 'No encontramos coincidencias' }).waitFor();
  await vault.getByRole('button', { name: 'Ver toda la biblioteca' }).click();
  await vault.getByRole('button', { name: 'Transcripciones', exact: true }).click();
  assert.equal(await vault.locator('.entry-card').count(), 1);
  assert.match(await vault.locator('.entry-card h2').innerText(), /Entrevista inicial/);
  await vault.getByRole('button', { name: 'Todo el Vault', exact: true }).click();
  await vault.getByLabel('Ordenar entradas').selectOption('name');
  assert.match(await vault.locator('.entry-card h2').first().innerText(), /Contrato/);

  // 5. Detalle: huella, exportación descifrada y borrado definitivo.
  await vault.getByRole('button', { name: 'Ver detalle de Entrevista inicial.wav', exact: true }).click();
  await vault.locator('.detail-panel').waitFor();
  assert.match(await vault.locator('.detail-panel').innerText(), /Huella/);
  await page.screenshot({ path: 'artifacts/vault/05-detail.png' });
  await vault.getByRole('button', { name: 'Exportar descifrado' }).click();
  await vault.getByText('Copia descifrada guardada en: /tmp/Entrevista inicial.wav').waitFor();
  const exports = await page.evaluate(() => window.__exports);
  assert.equal(exports.length, 1);
  assert.equal(exports[0].id, '66666666-7777-4888-8999-aaaaaaaaaaaa');
  await vault.getByRole('button', { name: 'Eliminar del Vault' }).click();
  await dialog.waitFor();
  await page.keyboard.press('Escape');
  await page.waitForFunction(() => document.activeElement?.textContent?.includes('Eliminar del Vault'));
  await vault.getByRole('button', { name: 'Eliminar del Vault' }).click();
  await dialog.getByRole('button', { name: 'Eliminar definitivamente' }).click();
  await vault.getByText('se ha eliminado del Vault. No hay papelera.', { exact: false }).waitFor();
  assert.equal(await vault.locator('.entry-card').count(), 1);
  assert.deepEqual(await page.evaluate(() => window.__vault.entries.map(item => item.title)), ['Contrato de servicios.pdf']);

  // 6. Seguridad: estado real del equipo, copia y restauración.
  await vault.getByRole('button', { name: 'Seguridad y acceso', exact: true }).click();
  await vault.getByRole('heading', { name: 'Seguridad y acceso' }).waitFor();
  const security = await vault.locator('.security-grid').innerText();
  assert.match(security, /FileVault\s*Activado/);
  assert.match(security, /Clave ligada al hardware\s*Pendiente/);
  assert.match(security, /XChaCha20-Poly1305/);
  await page.screenshot({ path: 'artifacts/vault/06-security.png' });
  await vault.getByRole('button', { name: 'Crear copia' }).click();
  await dialog.getByRole('button', { name: 'Guardar copia' }).click();
  await vault.getByText('Copia cifrada guardada en: /tmp/Mi Vault.bluedoor').waitFor();
  assert.equal(await page.evaluate(() => window.__vault.backups), 1);

  await vault.getByRole('button', { name: 'Restaurar una copia' }).click();
  const restore = dialog.getByRole('button', { name: 'Restaurar', exact: true });
  assert(await restore.isDisabled(), 'restoring over an existing Vault needs explicit confirmation');
  await dialog.getByLabel('Contraseña o frase de recuperación').fill('frase-de-paso-larga');
  await dialog.getByRole('checkbox').check();
  await restore.click();
  await vault.getByText('Vault restaurado desde la copia.').waitFor();
  assert.equal(await vault.locator('.entry-card').count(), 2, 'the backup brought both entries back');
  assert.equal(await page.evaluate(() => window.__vault.restores), 1);

  // 7. Traslado de transcripciones.
  await vault.getByRole('button', { name: 'Seguridad y acceso', exact: true }).click();
  await vault.getByRole('button', { name: 'Trasladar', exact: true }).click();
  await page.screenshot({ path: 'artifacts/vault/07-migrate.png' });
  await dialog.locator('.modal-footer').getByRole('button', { name: 'Trasladar' }).click();
  await vault.getByText('2 transcripciones trasladadas y verificadas. Quedan 0 en el historial local.').waitFor();

  // 8. Bloqueo automático explícito y desbloqueo con contraseña.
  await vault.getByRole('button', { name: 'Bloquear Vault', exact: true }).click();
  await vault.getByRole('heading', { name: 'Lo tuyo, bajo llave.' }).waitFor();
  assert.equal(await vault.locator('.entry-card').count(), 0, 'locking hides every title');
  assert(!(await page.evaluate(() => document.body.innerText)).includes('Contrato de servicios'), 'no title survives locking');
  await page.screenshot({ path: 'artifacts/vault/08-locked.png' });
  await vault.getByLabel('Contraseña del Vault').fill('incorrecta');
  await vault.getByRole('button', { name: 'Desbloquear' }).click();
  await vault.getByRole('alert').filter({ hasText: 'La contraseña no es correcta.' }).waitFor();
  await vault.getByLabel('Contraseña del Vault').fill('frase-de-paso-larga');
  await vault.getByRole('button', { name: 'Desbloquear' }).click();
  await vault.getByRole('heading', { name: 'Todo el Vault' }).waitFor();
  assert.equal(await vault.locator('.entry-card').count(), 4);

  // 9. Recuperación con la frase y comprobación desde Seguridad.
  await vault.getByRole('button', { name: 'Bloquear Vault', exact: true }).click();
  await vault.getByRole('button', { name: 'Usar frase de recuperación' }).click();
  await dialog.getByLabel('Frase de recuperación').fill('palabra equivocada');
  await dialog.getByRole('button', { name: 'Desbloquear' }).click();
  await dialog.getByText('La frase de recuperación no es válida.').waitFor();
  await dialog.getByLabel('Frase de recuperación').fill(phrase);
  await dialog.getByRole('button', { name: 'Desbloquear' }).click();
  await vault.getByText('Vault desbloqueado con la frase de recuperación.').waitFor();
  await vault.getByRole('button', { name: 'Seguridad y acceso', exact: true }).click();
  await vault.getByRole('button', { name: 'Usar la frase ahora' }).click();
  await dialog.getByLabel('Frase de recuperación').fill(phrase);
  await dialog.getByRole('button', { name: 'Comprobar' }).click();
  await vault.getByText('La frase de recuperación es válida para este Vault.').waitFor();

  // 10. Sin secretos ni datos en el almacenamiento web.
  const finalStorage = await page.evaluate(() => JSON.stringify(localStorage));
  assert.equal(finalStorage, storage, 'the Vault writes nothing to localStorage');
  assert(!finalStorage.includes(phrase) && !finalStorage.includes('frase-de-paso-larga'));
  const commands = await page.evaluate(() => window.__commands);
  for (const expected of [
    'vault_status', 'vault_create', 'vault_list', 'vault_import', 'vault_export_entry', 'vault_delete_entry',
    'vault_backup', 'vault_restore', 'vault_migrate_transcripts', 'vault_lock', 'vault_unlock', 'vault_unlock_recovery',
  ]) {
    assert(commands.includes(expected), `expected command ${expected}`);
  }

  // 11. Diseño adaptable sin desbordes.
  for (const [width, height] of [[1440, 960], [1280, 828], [1150, 820], [960, 680]]) {
    await page.setViewportSize({ width, height });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `no window overflow at ${width}`);
    assert(await vault.evaluate(element => element.scrollWidth <= element.clientWidth), `no vault overflow at ${width}`);
    for (const card of await vault.locator('.entry-card').all()) {
      const box = await card.boundingBox();
      assert(box.x + box.width <= width + 1, `card fits at ${width}`);
    }
    await page.screenshot({ path: `artifacts/vault/09-responsive-${width}.png` });
  }

  assert.deepEqual(errors, []);
  console.log('Vault: creación, frase, importación, búsqueda, detalle, exportación, borrado, copia, restauración, traslado, bloqueo y recuperación verificados.');
} finally {
  await browser.close();
}
