#!/usr/bin/env node
// Contract: pipeforge lib/herdr-session-recipes.md § Worktree provisioning.
import { spawnSync } from 'node:child_process';
import { constants, closeSync, lstatSync, mkdirSync, openSync, readFileSync, realpathSync, writeFileSync, writeSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';

const root = realpathSync(resolve(dirname(fileURLToPath(import.meta.url)), '..'));
const lane = process.env.LANE ?? '';
const base = process.env.BASE ?? '';
const endpoint = 'unix:///var/run/docker.sock';
const pipeforge = '/root/repos/pipeforge/plugin/bin/pipeforge';
const pgCommand = ['docker', '--host', endpoint, 'exec', '-i', 'postgres', 'psql', '-U', 'postgres'];
const privateValues = new Set([process.env.PIPEFORGE_LANE_RESOURCE_HANDLE, process.env.WORKTREE_DB_PASSWORD].filter(Boolean));
let logPath;
const check = (condition, message) => { if (!condition) throw new Error(message); };
const exists = path => { try { lstatSync(path); return true; } catch (error) { if (error.code === 'ENOENT') return false; throw error; } };
function redact(message) {
  let result = String(message);
  for (const value of privateValues) result = result.replaceAll(value, '[redacted]');
  return result.replace(/postgres(?:ql)?:\/\/[^\s"']+/gi, '[redacted database URL]')
    .replace(/((?:\\?["'])?(?:owner|ownership)[_-]?token(?:\\?["'])?\s*[:=]\s*)(?:"(?:\\.|[^"\\])*"?|'(?:\\.|[^'\\])*'?|[^\s,}\]]+)/gi, '$1"[redacted]"')
    .replace(/[a-f0-9]{64}/gi, '[redacted]');
}
function diagnose(message) {
  const text = redact(message);
  process.stderr.write(`${text}\n`);
  if (logPath) {
    const fd = openSync(logPath, constants.O_WRONLY | constants.O_APPEND | constants.O_CREAT | constants.O_NOFOLLOW, 0o600);
    try { writeSync(fd, `${new Date().toISOString()} ${text}\n`); } finally { closeSync(fd); }
  }
}
function run(command, args, { cwd = root, input, label = command } = {}) {
  diagnose(`Step: ${label}`);
  const result = spawnSync(command, args, { cwd, input, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  // argv can contain the private registration handle; log only the step label.
  check(!result.error && result.status === 0,
    `${label} failed (${result.status ?? result.error?.code ?? result.signal})\n${result.error?.message ?? ''}\nstdout:\n${result.stdout ?? ''}\nstderr:\n${result.stderr ?? ''}`);
  if (result.stderr) diagnose(`${label} stderr:\n${result.stderr}`);
  return result.stdout.trim();
}
const docker = (args, options) => run('docker', ['--host', endpoint, ...args], options);
const pg = sql => docker(['exec', '-i', 'postgres', 'psql', '-X', '--no-password', '-U', 'postgres', '-d', 'postgres', '-v', 'ON_ERROR_STOP=1', '-At', '-f', '-'], { input: sql, label: 'lane PostgreSQL operation' });
const writeNew = (path, content) => writeFileSync(path, content, { flag: 'wx', mode: 0o600 });
function render(template, replacements) {
  let result = template;
  for (const [key, value] of Object.entries(replacements)) result = result.replaceAll(`__${key}__`, value);
  check(!/__[A-Z][A-Z_]*__/.test(result), 'Unresolved Compose template field');
  return result;
}

function registrationContext(destination) {
  check(process.env.PIPEFORGE_LANE_RESOURCE_HANDLE, 'PIPEFORGE_LANE_RESOURCE_HANDLE required; invoke through guarded provisioning');
  const handle = JSON.parse(process.env.PIPEFORGE_LANE_RESOURCE_HANDLE);
  check(handle.version === 1 && handle.laneDestination === destination && typeof handle.recordPath === 'string', 'Resource handle does not bind this lane');
  const retained = JSON.parse(readFileSync(handle.recordPath, 'utf8'));
  const ownership = retained.ownership;
  check(ownership?.lane.primaryRoot === root && ownership.lane.destination === destination && ownership.lane.branch === lane &&
    ownership.operationId === handle.operationId && ownership.owner === handle.owner && ownership.state === 'allocating', 'Resource handle differs from allocating ownership');
  for (const resource of ownership.resources) if (resource.ownerToken) privateValues.add(resource.ownerToken);
  const scratch = ownership.resources.find(resource => resource.id === ownership.scratchRoot);
  check(scratch?.kind === 'filesystem' && scratch.scope === 'subtree' && scratch.state === 'observed' && realpathSync(scratch.locator) === scratch.locator, 'Observed canonical scratch required');
  const stat = lstatSync(scratch.locator, { bigint: true });
  check(stat.isDirectory() && stat.uid === BigInt(process.getuid()) && (stat.mode & 0o777n) === 0o700n &&
    String(stat.dev) === scratch.identity.dev && String(stat.ino) === scratch.identity.ino, 'Scratch identity differs');
  logPath = join(scratch.locator, 'worktree-create.log');
  diagnose(`Diagnostics: ${logPath}`);
  check(JSON.stringify(retained.resourceCommands?.['postgres-database']) === JSON.stringify(pgCommand), 'Configure deliver.worktreeResourceCommands.postgres-database as documented');
  return handle;
}

function main() {
  check(/^lane[a-z0-9]{1,20}$/.test(lane), 'LANE must match ^lane[a-z0-9]{1,20}$');
  const destination = join(root, '.worktrees', lane);
  const dryRun = process.env.DRY_RUN === '1';
  const handle = dryRun ? null : registrationContext(destination);
  check(base && !base.startsWith('-') && !/[\s~^:?*\[\\]/.test(base), 'BASE must be a safe commit ref');
  const oid = run('git', ['rev-parse', '--verify', `${base}^{commit}`], { label: 'resolve base' });
  check(!exists(destination), 'Lane destination already exists; inspect retained partial provisioning');
  check(!run('git', ['for-each-ref', '--format=%(refname)', `refs/heads/${lane}`], { label: 'check lane branch' }), 'Lane branch already exists');
  if (exists(join(root, '.worktrees'))) check(realpathSync(join(root, '.worktrees')) === join(root, '.worktrees'), '.worktrees must be canonical');
  const project = `profitofexile-${lane}`;
  const host = `${project}.dev.localhost`;
  const databases = [`profitofexile_${lane}`, `profitofexile_${lane}_integration`];
  const services = ['app', 'frontend', 'collector', 'desktop'];
  const names = Object.fromEntries(services.map(service => [service, `profitofexile_${lane}_${service}`]));
  const template = readFileSync(join(root, 'docker-compose.worktree.yml'), 'utf8');
  if (dryRun) {
    // Exercise the same template substitution without provider calls or writes.
    const sample = { LANE: lane, DATABASE_URL: '"dry-run"' };
    for (const service of services) {
      sample[`${service.toUpperCase()}_IMAGE`] = 'dry-run';
      sample[`${service.toUpperCase()}_TOKEN`] = 'dry-run';
    }
    const rendered = render(template, sample);
    for (const name of Object.values(names)) check(rendered.includes(`container_name: ${name}`), 'Template container inventory differs');
    console.log(JSON.stringify({ dryRun: true, destination, branch: lane, baseOid: oid, project, host, databases, containers: names,
      ports: [], volumes: [], networks: ['shared external infra'], effects: 'none; registration, Docker and PostgreSQL not contacted' }, null, 2));
    return;
  }
  check(process.env.WORKTREE_DB_PASSWORD, 'WORKTREE_DB_PASSWORD required for the existing local profitofexile role');
  const allocations = [];
  function resource(action, args = []) {
    const output = run(pipeforge, ['herdr-worktree', '--action', action, '--repo', root, '--out', handle.recordPath,
      '--handle', JSON.stringify(handle), ...args], { label: `registration ${action}` });
    let response;
    try { response = JSON.parse(output); } catch { throw new Error(`Invalid registration JSON:\n${output}`); }
    check(response.status === 'success', `Registration refused:\n${output}`);
    return response.data;
  }
  function intent(id, kind, locator, dependsOn = []) {
    const allocationId = randomUUID();
    const data = resource('resource-intent', ['--resource', JSON.stringify({ id, allocationId, kind, locator, scope: 'object', dependsOn })]);
    check(typeof data.ownerToken === 'string' && /^[a-f0-9]{64}$/.test(data.ownerToken), 'Registration returned no ownership token');
    privateValues.add(data.ownerToken);
    allocations.push(allocationId);
    return { allocationId, token: data.ownerToken };
  }
  const observe = allocation => resource('resource-observe', ['--allocation-id', allocation.allocationId]);

  const daemonId = docker(['info', '--format', '{{.ID}}']);
  check(daemonId, 'Docker daemon identity missing');
  docker(['network', 'inspect', 'infra']);
  const clusterId = pg('SELECT system_identifier::text FROM pg_control_system();');
  check(/^\d+$/.test(clusterId), 'PostgreSQL cluster identity unavailable');
  check(pg("SELECT 1 FROM pg_roles WHERE rolname = 'profitofexile';") === '1', 'Existing local profitofexile role required');
  for (const db of databases) check(pg(`SELECT 1 FROM pg_database WHERE datname = '${db}';`) === '', `Lane database exists: ${db}`);
  const existingNames = docker(['container', 'ls', '-a', '--format', '{{.Names}}']).split('\n');
  for (const name of Object.values(names)) check(!existingNames.includes(name), `Lane container exists: ${name}`);
  for (const kind of ['container', 'volume', 'network']) check(!docker([kind, 'ls', ...(kind === 'container' ? ['-a'] : []), '-q', '--filter', `label=com.docker.compose.project=${project}`]), 'Lane Compose project exists');
  const replacements = { LANE: lane };
  for (const service of services) {
    const image = docker(['container', 'inspect', `profitofexile_${service}`, '--format', '{{.Image}}']);
    check(/^sha256:[a-f0-9]{64}$/.test(image), `Main ${service} image required; build main first`);
    const rows = JSON.parse(docker(['image', 'inspect', image]));
    check(rows.length === 1 && !Object.keys(rows[0].Config.Volumes ?? {}).length, 'Image-declared volumes require explicit registration support');
    replacements[`${service.toUpperCase()}_IMAGE`] = image;
  }
  // Older bases carry the original guard. Update only that guard in the lane;
  // the requested base commit and all other source bytes stay authoritative.
  const guardPath = 'scripts/lib/integration-env.sh';
  const baseEnv = run('git', ['show', `${oid}:${guardPath}`], { label: 'inspect base integration guard' });
  const oldGuard = 'if [ "$DB_NAME" != profitofexile_integration ]; then\n\techo "FAIL: database rule requires profitofexile_integration; other INTEGRATION_DB values are refused." >&2\n\texit 1\nfi';
  const newGuard = readFileSync(join(root, guardPath), 'utf8').match(/if \[\[ ! "\$\{INTEGRATION_DB-\$DB_NAME\}"[^]*?\nfi/)?.[0];
  check(newGuard && (baseEnv.includes(oldGuard) || baseEnv.includes(newGuard)), 'Unknown base integration guard; refusing unsafe adaptation');

  mkdirSync(join(root, '.worktrees'), { recursive: true });
  run('git', ['worktree', 'add', '-b', lane, destination, oid], { label: 'create lane worktree' });
  mkdirSync(join(destination, '.worktree'), { mode: 0o700 });
  for (const cache of ['go-pkg', 'frontend_node_modules', 'desktop_cargo_registry', 'desktop_cargo_git', 'desktop_target', 'desktop_node_modules']) mkdirSync(join(destination, '.worktree', cache));
  const ignorePath = join(destination, '.gitignore');
  const ignore = readFileSync(ignorePath, 'utf8');
  const missingIgnores = ['.worktrees/', '.worktree/', 'docker-compose.lane.yml'].filter(line => !ignore.split('\n').includes(line));
  if (missingIgnores.length) writeFileSync(ignorePath, `${ignore}\n${missingIgnores.join('\n')}\n`);
  if (baseEnv.includes(oldGuard)) {
    const path = join(destination, guardPath);
    writeFileSync(path, readFileSync(path, 'utf8').replace(oldGuard, newGuard));
  }
  for (const [index, db] of databases.entries()) {
    const allocation = intent(`database-${index}`, 'postgres-database', { clusterId, endpoint: 'postgres', database: db, maintenanceDatabase: 'postgres' });
    pg(`CREATE DATABASE "${db}" OWNER profitofexile;\nCOMMENT ON DATABASE "${db}" IS 'pipeforge-allocation:${allocation.token}';`);
    observe(allocation);
  }
  const containers = {};
  for (const service of services) {
    containers[service] = intent(`container-${service}`, 'docker-container', { daemonId, endpoint, name: names[service] }, ['database-0', 'database-1']);
    replacements[`${service.toUpperCase()}_TOKEN`] = containers[service].token;
  }
  const dbURL = new URL(`postgresql://postgres:5432/${databases[0]}`);
  dbURL.username = 'profitofexile';
  dbURL.password = process.env.WORKTREE_DB_PASSWORD;
  dbURL.searchParams.set('sslmode', 'disable');
  privateValues.add(dbURL.href);
  privateValues.add(dbURL.password);
  // Render a fixed lane URL: a caller's exported DATABASE_URL must never
  // redirect lane services into the main database through Compose precedence.
  replacements.DATABASE_URL = JSON.stringify(dbURL.href);
  writeNew(join(destination, 'docker-compose.lane.yml'), render(template, replacements));
  writeNew(join(destination, '.env'), `COMPOSE_FILE=docker-compose.lane.yml\nCOMPOSE_PROJECT_NAME=${project}\nINTEGRATION_DB=${databases[1]}\n`);
  writeNew(join(destination, '.worktree/db-password'), dbURL.password);
  // Make includes .env.local, while Compose reads .env. Export the lane test
  // database into the host integration script before it sources its guard.
  writeNew(join(destination, '.env.local'), `export INTEGRATION_DB := ${databases[1]}\nexport INTEGRATION_DB_PASSWORD := $(shell cat .worktree/db-password)\nexport COMPOSE_FILE := docker-compose.lane.yml\nexport COMPOSE_PROJECT_NAME := ${project}\n`);
  writeNew(join(destination, '.worktree/vite.config.ts'), `import config from '../frontend/vite.config';\nexport default { ...config, server: { ...config.server, allowedHosts: ['${host}'], proxy: { '/api': { target: 'http://${names.app}:8080', changeOrigin: true } } } };\n`);
  const compose = args => docker(['compose', '--project-directory', destination, '--env-file', join(destination, '.env'), '-p', project,
    '-f', join(destination, 'docker-compose.lane.yml'), ...args], { cwd: destination, label: `lane compose ${args[0]}` });
  const config = JSON.parse(compose(['config', '--format', 'json']));
  check(Object.keys(config.services).sort().join(',') === services.slice().sort().join(','), 'Unexpected Compose service');
  check(!Object.keys(config.volumes ?? {}).length, 'Unregistered named volumes');
  check(Object.keys(config.networks).join(',') === 'infra' && config.networks.infra.external === true, 'Only shared infra network supported');
  for (const [service, entry] of Object.entries(config.services)) {
    check(entry.container_name === names[service] && !entry.ports?.length, 'Unexpected container name or host port');
    check(entry.volumes.every(volume => volume.type === 'bind' && (volume.source === destination || volume.source.startsWith(`${destination}/`))), 'Foreign mutable mount');
  }
  compose(['create', '--no-build', ...services]);
  for (const allocation of Object.values(containers)) observe(allocation);
  const group = intent('compose', 'compose-project', { daemonId, endpoint, name: project }, services.map(service => `container-${service}`));
  observe(group);
  // Run migrations without allocating an unrecorded Compose run container.
  compose(['start', 'app']);
  compose(['exec', '-T', 'app', 'go', 'run', './cmd/migrate', 'up']);
  compose(['start', 'frontend', 'collector']);
  resource('resources-complete', ['--allocations', JSON.stringify(allocations)]);
  console.log(`Created ${project}: https://${host}`);
}

try { main(); } catch (error) {
  try { diagnose(`worktree-create: ${error.message}\nPartial resources retained; inspect the operation record before recovery.${logPath ? `\nLog: ${logPath}` : '\nNo validated scratch log available.'}`); }
  catch { process.stderr.write('worktree-create failed; diagnostic log unavailable\n'); }
  process.exitCode = 1;
}
