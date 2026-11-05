import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const catalogDirectory = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../src/locales');

export function leaves(object, source, prefix = '', result = new Map()) {
  if (object === null || typeof object !== 'object' || Array.isArray(object)) {
    throw new Error(`${source}: ${prefix || '<root>'} must be a JSON object`);
  }
  for (const [key, value] of Object.entries(object)) {
    const name = prefix ? `${prefix}.${key}` : key;
    if (typeof value === 'string') {
      if (!value.trim()) throw new Error(`${source}: ${name} has an empty string`);
      if (result.has(name)) throw new Error(`${source}: ambiguous dot-path ${name}`);
      result.set(name, value);
    } else {
      leaves(value, source, name, result);
    }
  }
  return result;
}

export async function readCatalog(file) {
  let object;
  try {
    object = JSON.parse(await fs.readFile(file, 'utf8'));
  } catch (error) {
    throw new Error(`${file}: ${error.message}`);
  }
  leaves(object, file);
  return object;
}

export function checkParity(en, zh) {
  const errors = [];
  const onlyEn = [...en.keys()].filter(key => !zh.has(key)).sort((a, b) => a.localeCompare(b));
  const onlyZh = [...zh.keys()].filter(key => !en.has(key)).sort((a, b) => a.localeCompare(b));
  if (onlyEn.length) errors.push(`Only in en (missing from zh): ${onlyEn.join(', ')}`);
  if (onlyZh.length) errors.push(`Only in zh (missing from en): ${onlyZh.join(', ')}`);
  const placeholders = value => [...new Set([...value.matchAll(/\{\{\s*([^{}]+?)\s*\}\}/g)].map(match => match[1].trim()))].sort();
  for (const [key, value] of en) {
    if (!zh.has(key)) continue;
    const english = placeholders(value);
    const chinese = placeholders(zh.get(key));
    if (JSON.stringify(english) !== JSON.stringify(chinese)) {
      errors.push(`Placeholder mismatch at ${key}: en=${JSON.stringify(english)}, zh=${JSON.stringify(chinese)}`);
    }
  }
  if (errors.length) throw new Error(`i18n parity failed:\n${errors.join('\n')}`);
}

export async function check() {
  const en = leaves(await readCatalog(path.join(catalogDirectory, 'en.json')), 'en.json');
  const zh = leaves(await readCatalog(path.join(catalogDirectory, 'zh.json')), 'zh.json');
  checkParity(en, zh);
  for (const [key, value] of zh) {
    if (value === en.get(key) && /^[\x00-\x7f]*$/.test(value) && /[A-Za-z]/.test(value)) {
      console.warn(`WARN: zh.json ${key} equals en.json: ${JSON.stringify(value)}`);
    }
  }
  console.log(`i18n check: en=${en.size} keys, zh=${zh.size} keys; parity OK`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await check().catch(error => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
