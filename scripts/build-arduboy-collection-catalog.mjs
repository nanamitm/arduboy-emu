#!/usr/bin/env node
// Generate a browser-friendly catalog from a local clone of
// https://github.com/eried/ArduboyCollection.
import { mkdir, readdir, readFile, writeFile } from 'node:fs/promises';
import { join, relative, sep } from 'node:path';
import { execFileSync } from 'node:child_process';

const args = process.argv.slice(2);
const value = (name) => args[args.indexOf(name) + 1];
const source = value('--source');
const output = value('--output');
if (!source || !output) {
  console.error('Usage: build-arduboy-collection-catalog.mjs --source <clone> --output <catalog.json>');
  process.exit(2);
}

const RAW_BASE = 'https://raw.githubusercontent.com/eried/ArduboyCollection/master/';
const sourceCommit = execFileSync('git', ['-C', source, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
const ignoredDirectories = new Set(['.git', '.github', 'docs']);
const rawUrl = (path) => RAW_BASE + path.split(sep).map(encodeURIComponent).join('/');

async function walk(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  const result = [];
  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (!ignoredDirectories.has(entry.name)) result.push(...await walk(join(dir, entry.name)));
    } else {
      result.push(join(dir, entry.name));
    }
  }
  return result;
}

function parseIni(text) {
  const values = {};
  for (const line of text.split(/\r?\n/)) {
    const match = line.match(/^\s*([^=;#][^=]*)=(.*)$/);
    if (match) values[match[1].trim().toLowerCase()] = match[2].trim();
  }
  return values;
}

function parseShortcut(text) {
  const match = text.match(/^URL=(.+)$/mi);
  return match ? match[1].trim() : '';
}

const files = await walk(source);
const byDirectory = new Map();
for (const file of files) {
  const directory = file.slice(0, file.lastIndexOf(sep));
  if (!byDirectory.has(directory)) byDirectory.set(directory, []);
  byDirectory.get(directory).push(file);
}

const games = [];
for (const [directory, entries] of byDirectory) {
  const ini = entries.find((file) => file.toLowerCase().endsWith(`${sep}game.ini`));
  if (!ini) continue;
  const relDir = relative(source, directory);
  const [category] = relDir.split(sep);
  if (!category || ignoredDirectories.has(category)) continue;
  const metadata = parseIni(await readFile(ini, 'utf8'));
  const localHex = entries.find((file) => /\.(hex|arduboy)$/i.test(file));
  const configuredHex = metadata.hex;
  if (!localHex && !configuredHex) continue;
  const image = entries.find((file) => /\.(png|jpe?g|webp)$/i.test(file));
  const details = entries.find((file) => /\.url$/i.test(file) && !/source\.url$/i.test(file));
  const sourceUrl = entries.find((file) => /source\.url$/i.test(file));
  const rel = (file) => relative(source, file);
  const title = metadata.title || relDir.split(sep).at(-1);
  games.push({
    id: relDir.replaceAll(sep, '/'),
    title,
    author: metadata.author || '',
    description: metadata.description || '',
    license: metadata.license || '',
    category,
    hexUrl: configuredHex || rawUrl(rel(localHex)),
    imageUrl: image ? rawUrl(rel(image)) : '',
    detailsUrl: details ? parseShortcut(await readFile(details, 'utf8')) : '',
    sourceUrl: sourceUrl ? parseShortcut(await readFile(sourceUrl, 'utf8')) : '',
  });
}

games.sort((a, b) => a.title.localeCompare(b.title, 'en'));
await mkdir(output.slice(0, output.lastIndexOf(sep)), { recursive: true });
await writeFile(output, `${JSON.stringify({
  source: 'https://github.com/eried/ArduboyCollection',
  sourceCommit,
  games,
}, null, 2)}\n`);
console.log(`Wrote ${games.length} games to ${output}`);
