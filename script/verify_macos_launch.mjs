// Exercise the shipped bundle with disposable data, never the installed app.
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdtempSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

if (process.platform !== 'darwin') throw new Error('macOS is required');
if (!process.argv[2]) throw new Error('Usage: node script/verify_macos_launch.mjs /path/to/CareerOS.app');
const bundle = realpathSync(process.argv[2]);
const executable = execFileSync('/usr/libexec/PlistBuddy', ['-c', 'Print :CFBundleExecutable', join(bundle, 'Contents/Info.plist')], { encoding: 'utf8' }).trim();
if (executable !== 'CareerOS') throw new Error(`Unexpected executable: ${executable}`);
const binary = join(bundle, 'Contents/MacOS', executable);
const root = mkdtempSync(join(tmpdir(), 'careeros-launch-'));
console.log(`Test data and diagnostics: ${root}`);
execFileSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', bundle], { stdio: 'inherit' });
for (const name of ['codex', 'typst']) {
  execFileSync(join(bundle, 'Contents/Resources/resources/runtime', name), ['--version'], { stdio: 'inherit', timeout: 15000 });
}

const matchingPids = () => execFileSync('/bin/ps', ['-axo', 'pid=,command='], { encoding: 'utf8' })
  .split('\n').flatMap(line => {
    const match = line.trim().match(/^(\d+)\s+(.*)$/);
    return match && match[2] === binary ? [Number(match[1])] : [];
  });

for (const mode of ['direct', 'launch-services']) {
  const data = join(root, mode);
  const previous = new Set(matchingPids());
  let child;
  let result;
  try {
    if (mode === 'direct') {
      child = spawn(binary, [], { env: { ...process.env, CAREEROS_DATA_DIR: data }, stdio: 'inherit' });
      child.on('error', error => { result = String(error); });
      child.on('exit', (code, signal) => { result = `code=${code}, signal=${signal}`; });
    } else {
      execFileSync('/usr/bin/open', ['-n', '-g', '--env', `CAREEROS_DATA_DIR=${data}`, bundle], { timeout: 15000 });
    }
    // Check repeatedly so a delayed OS kill cannot pass a momentary PID check.
    for (let second = 0; second < 15; second++) {
      await delay(1000);
      if (result !== undefined) throw new Error(`${mode} exited: ${result}`);
      if (second >= 4 && !matchingPids().some(pid => !previous.has(pid))) {
        throw new Error(`${mode}: no running bundle process`);
      }
    }
    if (!existsSync(join(data, 'database/careeros.sqlite3'))) throw new Error(`${mode}: database initialization did not finish`);
    console.log(`${mode}: launch and initialization passed`);
  } finally {
    for (const pid of matchingPids().filter(pid => !previous.has(pid))) {
      try { process.kill(pid, 'SIGTERM'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
    }
  }
}
