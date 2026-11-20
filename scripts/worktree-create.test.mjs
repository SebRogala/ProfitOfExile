// Run: node --test scripts/worktree-create.test.mjs
// Exercises patchLaneFiles and checkBaseTrackedFiles from worktree-create.mjs
// against scratch git repos outside the checkout; no database, container,
// Compose project or lane.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { LANE_NAME_PATTERN, checkBaseTrackedFiles, currentGuards, patchLaneFiles, render } from './worktree-create.mjs';

const oldGuard = 'if [ "$DB_NAME" != profitofexile_integration ]; then\n\texit 1\nfi';
const newGuard = 'if [[ ! "${INTEGRATION_DB-$DB_NAME}" =~ lane ]]; then\n\texit 1\nfi';
const guard = { oldGuard, newGuard };
const ignoreBody = 'node_modules/\n';
const envBody = `#!/bin/bash\n${oldGuard}\necho ok\n`;
const patchedIgnore = `${ignoreBody}\n.worktrees/\n.worktree/\ndocker-compose.lane.yml\n`;
const patchedEnv = `#!/bin/bash\n${newGuard}\necho ok\n`;
const ENV_PATH = 'scripts/lib/integration-env.sh';

const scratchDir = () => realpathSync(mkdtempSync(join(tmpdir(), 'poe-worktree-create-test-')));
function git(cwd, ...args) {
  const result = spawnSync('git', args, { cwd, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
}

// Builds a committed base. `links` names tracked paths committed as symlinks
// to files outside the repo; returns the repo and each link target.
function scratchLane(links = []) {
  const scratch = scratchDir();
  const lane = join(scratch, 'lane');
  mkdirSync(join(lane, 'scripts', 'lib'), { recursive: true });
  const targets = {};
  for (const [path, body] of [['.gitignore', ignoreBody], [ENV_PATH, envBody]]) {
    if (links.includes(path)) {
      targets[path] = join(scratch, `outside-${path.replaceAll('/', '-')}`);
      writeFileSync(targets[path], body);
      symlinkSync(targets[path], join(lane, path));
    } else writeFileSync(join(lane, path), body);
  }
  git(lane, 'init', '-q');
  git(lane, 'add', '.');
  git(lane, '-c', 'user.name=test', '-c', 'user.email=test@example.invalid', 'commit', '-qm', 'base');
  return { lane, targets };
}

test('symlinked .gitignore is refused and its target keeps its bytes', () => {
  const { lane, targets } = scratchLane(['.gitignore']);
  assert.throws(() => patchLaneFiles(lane, guard),
    { message: `Refusing lane write through a symlink: ${join(lane, '.gitignore')}` });
  assert.equal(readFileSync(targets['.gitignore'], 'utf8'), ignoreBody);
});

test('symlinked integration-env.sh is refused and its target keeps its bytes', () => {
  const { lane, targets } = scratchLane([ENV_PATH]);
  assert.throws(() => patchLaneFiles(lane, guard), { message: `Refusing lane write through a symlink: ${join(lane, ENV_PATH)}` });
  assert.equal(readFileSync(targets[ENV_PATH], 'utf8'), envBody);
});

// No path check precedes the open, so a static directory symlink reaches the
// same post-open descriptor check a directory swapped mid-call would.
test('integration guard reached through a symlinked directory is refused and the outside file keeps its bytes', () => {
  const scratch = scratchDir();
  const lane = join(scratch, 'lane');
  const outside = join(scratch, 'outside-lib');
  mkdirSync(join(lane, 'scripts'), { recursive: true });
  mkdirSync(outside);
  writeFileSync(join(outside, 'integration-env.sh'), envBody);
  writeFileSync(join(lane, '.gitignore'), ignoreBody);
  symlinkSync(outside, join(lane, 'scripts', 'lib'));
  assert.throws(() => patchLaneFiles(lane, guard), { message: `Refusing lane write through a symlinked directory: ${join(lane, ENV_PATH)}` });
  assert.equal(readFileSync(join(outside, 'integration-env.sh'), 'utf8'), envBody);
});

test('regular .gitignore receives the missing lane ignores', () => {
  const { lane } = scratchLane();
  patchLaneFiles(lane, guard);
  assert.equal(readFileSync(join(lane, '.gitignore'), 'utf8'), patchedIgnore);
});

test('regular integration-env.sh receives the replaced guard', () => {
  const { lane } = scratchLane();
  patchLaneFiles(lane, guard);
  assert.equal(readFileSync(join(lane, ENV_PATH), 'utf8'), patchedEnv);
});

test('.gitignore already carrying the lane ignores is left byte-identical', () => {
  const { lane } = scratchLane();
  patchLaneFiles(lane, guard);
  patchLaneFiles(lane, null);
  assert.equal(readFileSync(join(lane, '.gitignore'), 'utf8'), patchedIgnore);
});

test('integration-env.sh is left byte-identical when no guard replacement is requested', () => {
  const { lane } = scratchLane();
  patchLaneFiles(lane, null);
  assert.equal(readFileSync(join(lane, ENV_PATH), 'utf8'), envBody);
});

test('base commit with a symlinked .gitignore is refused naming the path', () => {
  const { lane } = scratchLane(['.gitignore']);
  assert.throws(() => checkBaseTrackedFiles(lane, 'HEAD'), { message: 'Refusing base commit: .gitignore is a symlink' });
});

test('base commit with a symlinked integration-env.sh is refused naming the path', () => {
  const { lane } = scratchLane([ENV_PATH]);
  assert.throws(() => checkBaseTrackedFiles(lane, 'HEAD'), { message: `Refusing base commit: ${ENV_PATH} is a symlink` });
});

test('base commit with regular tracked files passes the preflight', () => {
  const { lane } = scratchLane();
  assert.doesNotThrow(() => checkBaseTrackedFiles(lane, 'HEAD'));
});

// Renders the committed template with synthetic values into a temp dir and
// asks Compose for its active services; nothing is created or started.
function renderedServices(...flags) {
  const template = readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'docker-compose.worktree.yml'), 'utf8');
  const sample = { LANE: 'lanetest', DATABASE_URL: '"postgresql://test@postgres:5432/test"' };
  for (const service of ['APP', 'FRONTEND', 'COLLECTOR', 'DESKTOP']) Object.assign(sample, { [`${service}_IMAGE`]: 'test', [`${service}_TOKEN`]: 'test' });
  const file = join(scratchDir(), 'docker-compose.lane.yml');
  writeFileSync(file, render(template, sample));
  const result = spawnSync('docker', ['compose', ...flags, '-f', file, 'config', '--services'], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim().split('\n').sort();
}

test('default rendered lane compose excludes the collector', () => {
  assert.deepEqual(renderedServices(), ['app', 'desktop', 'frontend']);
});

test('collector profile opt-in adds the collector to the rendered lane compose', () => {
  assert.deepEqual(renderedServices('--profile', 'collector'), ['app', 'collector', 'desktop', 'frontend']);
});

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const laneNameHome = () => readFileSync(join(repoRoot, 'scripts/lib/lane-name.sh'), 'utf8');

// keep:critical - a mismatch lets worktree-create accept a lane the guards refuse, or the reverse.
test('keep:critical lane pattern equals scripts/lib/lane-name.sh', () => {
  const body = laneNameHome().match(/^LANE_NAME_BODY='([^']+)'$/m)?.[1];
  assert.ok(body, 'LANE_NAME_BODY not found in scripts/lib/lane-name.sh');
  assert.equal(LANE_NAME_PATTERN.source, `^${body}$`);
});

// keep:critical - AC-3 provisioning path for lanes cut from bases older than lane-name.sh.
test('keep:critical guard patch from a base without lane-name.sh still accepts the lane database', () => {
  const originalGuard = 'if [ "$DB_NAME" != profitofexile_integration ]; then\n\techo "FAIL: database rule requires profitofexile_integration; other INTEGRATION_DB values are refused." >&2\n\texit 1\nfi';
  const lane = join(scratchDir(), 'lane');
  mkdirSync(join(lane, 'scripts', 'lib'), { recursive: true });
  writeFileSync(join(lane, '.gitignore'), ignoreBody);
  writeFileSync(join(lane, ENV_PATH), `#!/usr/bin/env bash\nDB_NAME="\${INTEGRATION_DB:-profitofexile_integration}"\n${originalGuard}\n`);
  const { newGuard } = currentGuards(repoRoot);
  patchLaneFiles(lane, { oldGuard: originalGuard, newGuard, laneName: laneNameHome() });
  const source = db => spawnSync('bash', ['-c', `source ${ENV_PATH}`], { cwd: lane, encoding: 'utf8', env: { ...process.env, INTEGRATION_DB: db } });
  const accepted = source('profitofexile_lanex1_integration');
  assert.equal(accepted.status, 0, accepted.stderr);
  const refused = source('profitofexile_integration_x');
  assert.equal(refused.status, 1);
  assert.match(refused.stderr, /^FAIL: database rule /);
});
