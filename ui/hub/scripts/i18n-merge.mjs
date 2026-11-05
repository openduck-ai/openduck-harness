import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { catalogDirectory, checkParity, leaves, readCatalog } from './i18n-check.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '../../..');
const fragmentDirectory = path.join(root, '.agent/i18n-fragments');
const ownersFile = path.join(fragmentDirectory, 'owners.json');
const lock = path.join(root, '.agent/i18n-merge.lock');
const PROTECTED = [
  'app.documentTitle',
  'app.errorBoundary.heading',
  'app.errorBoundary.message',
  'app.errorBoundary.reload',
  'header.projects',
  'header.running',
  'header.settings',
  'header.themeToggleTitle',
  'header.themeToggleLabel',
  'header.connectedTitle',
  'header.disconnectedTitle',
  'common.cancel',
  'common.save',
  'common.retry',
  'common.close',
  'common.loading',
  'language.label',
  'language.english',
  'language.chinese',
];

async function acquireLock() {
  for (let attempt = 0; attempt < 200; attempt++) {
    try {
      const handle = await fs.open(lock, 'wx');
      await handle.close();
      return;
    } catch (error) {
      if (error.code !== 'EEXIST') throw error;
      try {
        if (Date.now() - (await fs.stat(lock)).mtimeMs > 60_000) {
          await fs.rm(lock, { force: true });
          continue;
        }
      } catch (statError) {
        if (statError.code !== 'ENOENT') throw statError;
      }
      await delay(50);
    }
  }
  throw new Error(`Timed out acquiring ${lock}`);
}

async function loadOwners() {
  let content;
  try {
    content = await fs.readFile(ownersFile, 'utf8');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    console.warn(`WARN: ${ownersFile} not found; falling back to additive merge.`);
    return null;
  }
  let owners;
  try {
    owners = JSON.parse(content);
  } catch (error) {
    throw new Error(`${ownersFile}: ${error.message}`);
  }
  if (!owners || typeof owners !== 'object' || Array.isArray(owners)) {
    throw new Error(`${ownersFile} must be a JSON object mapping owner to prefix list`);
  }
  const prefixToOwner = new Map();
  for (const [owner, prefixes] of Object.entries(owners)) {
    if (!Array.isArray(prefixes)) {
      throw new Error(`${ownersFile}: prefixes for ${owner} must be an array`);
    }
    for (const prefix of prefixes) {
      if (typeof prefix !== 'string' || !prefix.trim()) {
        throw new Error(`${ownersFile}: invalid prefix for ${owner}`);
      }
      if (prefixToOwner.has(prefix)) {
        throw new Error(`Overlapping prefix "${prefix}" declared by both ${prefixToOwner.get(prefix)} and ${owner}`);
      }
      prefixToOwner.set(prefix, owner);
    }
  }
  return owners;
}

function validateFragmentPrefixes(fragmentLeaves, file, owner, owners) {
  if (!owners) return;
  if (!Object.hasOwn(owners, owner)) {
    throw new Error(`Fragment ${file} belongs to undeclared owner "${owner}" in owners.json`);
  }
  const prefixes = owners[owner];
  for (const [key] of fragmentLeaves) {
    const isOwned = prefixes.some(p => key === p || key.startsWith(`${p}.`));
    if (!isOwned) {
      throw new Error(`Fragment ${file} provides key "${key}" outside declared prefixes for ${owner}: ${prefixes.join(', ')}`);
    }
  }
}

function isProtected(name) {
  return PROTECTED.some(key => key === name || key.startsWith(`${name}.`) || name.startsWith(`${key}.`));
}

function pruneLeaf(object, pathParts) {
  if (pathParts.length === 0) return true;
  const [head, ...tail] = pathParts;
  if (!object || typeof object !== 'object' || !Object.hasOwn(object, head)) {
    return false;
  }
  if (tail.length === 0) {
    delete object[head];
    return true;
  }
  const deleted = pruneLeaf(object[head], tail);
  if (deleted && typeof object[head] === 'object' && Object.keys(object[head]).length === 0) {
    delete object[head];
  }
  return deleted;
}

function pruneCatalog(catalog, locale, files, owners) {
  if (!owners) return;
  const suffix = `.${locale}.json`;
  const presentOwners = new Set(
    files.filter(file => file.endsWith(suffix)).map(file => file.slice(0, -suffix.length))
  );
  const prunePrefixes = [];
  for (const owner of presentOwners) {
    if (owners[owner]) {
      prunePrefixes.push(...owners[owner]);
    }
  }
  if (prunePrefixes.length === 0) return;

  const catalogLeaves = leaves(catalog, `${locale}.json`);
  for (const [key] of catalogLeaves) {
    if (isProtected(key)) continue;
    if (prunePrefixes.some(prefix => key === prefix || key.startsWith(`${prefix}.`))) {
      pruneLeaf(catalog, key.split('.'));
    }
  }
}

function mergeFragments(target, fragment, source) {
  for (const [key, value] of fragment) {
    for (const [existing, previous] of target) {
      if ((existing === key && previous.value !== value) || existing.startsWith(`${key}.`) || key.startsWith(`${existing}.`)) {
        throw new Error(`Fragment conflict at ${key} (${existing}): ${previous.source} and ${source}`);
      }
    }
    target.set(key, { value, source });
  }
}

function applyFragment(catalog, fragment) {
  for (const [name, { value }] of fragment) {
    if (isProtected(name)) continue;
    const keys = name.split('.');
    let object = catalog;
    for (const key of keys.slice(0, -1)) {
      if (Object.hasOwn(object, key) && typeof object[key] === 'string') {
        throw new Error(`Catalog shape conflict at ${name}: would replace existing leaf ${key}`);
      }
      if (!Object.hasOwn(object, key)) {
        Object.defineProperty(object, key, { value: {}, enumerable: true, writable: true, configurable: true });
      }
      object = object[key];
    }
    if (Object.hasOwn(object, keys.at(-1)) && typeof object[keys.at(-1)] !== 'string') {
      throw new Error(`Catalog shape conflict at ${name}: would remove existing keys`);
    }
    Object.defineProperty(object, keys.at(-1), { value, enumerable: true, writable: true, configurable: true });
  }
}

function sorted(object) {
  if (typeof object === 'string') return object;
  return Object.fromEntries(Object.entries(object).sort(([a], [b]) => a.localeCompare(b)).map(([key, value]) => [key, sorted(value)]));
}

async function merge() {
  let files;
  try {
    files = await fs.readdir(fragmentDirectory);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    console.log('i18n merge: no fragments found; nothing to merge.');
    return;
  }
  if (!files.some(file => /\.(en|zh)\.json$/.test(file))) {
    console.log('i18n merge: no fragments found; nothing to merge.');
    return;
  }
  await acquireLock();
  try {
    const owners = await loadOwners();
    files = (await fs.readdir(fragmentDirectory)).filter(file => /\.(en|zh)\.json$/.test(file)).sort((a, b) => a.localeCompare(b));
    const fragments = { en: new Map(), zh: new Map() };
    for (const file of files) {
      const match = file.match(/^(.+)\.(en|zh)\.json$/);
      if (!match) continue;
      const [, owner, locale] = match;
      const fragmentLeaves = leaves(await readCatalog(path.join(fragmentDirectory, file)), file);
      validateFragmentPrefixes(fragmentLeaves, file, owner, owners);
      mergeFragments(fragments[locale], fragmentLeaves, file);
    }
    const catalogs = {};
    const before = {};
    const after = {};
    let changed = 0;
    for (const locale of ['en', 'zh']) {
      catalogs[locale] = await readCatalog(path.join(catalogDirectory, `${locale}.json`));
      before[locale] = leaves(catalogs[locale], `${locale}.json`);
      pruneCatalog(catalogs[locale], locale, files, owners);
      applyFragment(catalogs[locale], fragments[locale]);
      after[locale] = leaves(catalogs[locale], `${locale}.json`);
      for (const key of new Set([...before[locale].keys(), ...after[locale].keys()])) {
        if (before[locale].get(key) !== after[locale].get(key)) changed++;
      }
    }
    checkParity(after.en, after.zh);
    for (const locale of ['en', 'zh']) {
      await fs.writeFile(path.join(catalogDirectory, `${locale}.json`), `${JSON.stringify(sorted(catalogs[locale]), null, 2)}\n`);
    }
    console.log(`i18n merge: en=${after.en.size} keys, zh=${after.zh.size} keys, changed=${changed}`);
  } finally {
    await fs.rm(lock, { force: true });
  }
}

await merge().catch(error => {
  console.error(error.message);
  process.exitCode = 1;
});
