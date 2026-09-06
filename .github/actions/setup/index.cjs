'use strict';

// No npm dependencies or generated bundle. GitHub supplies Node; the runner's
// tar reads both cargo-dist's Unix tar.xz and Windows ZIP archives.
const fs = require('node:fs/promises');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { spawnSync } = require('node:child_process');

function versionOf(input) {
  const version = (input || '').trim().replace(/^(?:asm198x-v|v)/, '');
  if (version.length > 120 || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(version)) {
    throw new Error('version must be an exact release version, not latest, a range, or a branch');
  }
  const prerelease = version.split('+')[0].split('-').slice(1).join('-');
  if (prerelease.split('.').some(part => /^0\d+$/.test(part))) {
    throw new Error('numeric prerelease components must not have leading zeroes');
  }
  return version;
}

function releaseAsset(platform, arch) {
  const target = {
    'darwin-arm64': 'aarch64-apple-darwin',
    'darwin-x64': 'x86_64-apple-darwin',
    'linux-x64': 'x86_64-unknown-linux-gnu',
    'win32-x64': 'x86_64-pc-windows-msvc',
  }[`${platform}-${arch}`];
  if (!target) throw new Error(`no asm198x release archive for ${platform}/${arch}`);
  const binary = platform === 'win32' ? 'asm198x.exe' : 'asm198x';
  return {
    target,
    binary,
    archive: `asm198x-${target}.${platform === 'win32' ? 'zip' : 'tar.xz'}`,
    // cargo-dist's ZIP has no enclosing directory; its tarballs do.
    member: platform === 'win32' ? binary : `asm198x-${target}/${binary}`,
  };
}

function verifyChecksum(bytes, checksum, filename) {
  const fields = checksum.trim().split(/\s+/);
  if (fields.length !== 2 || !/^[a-fA-F0-9]{64}$/.test(fields[0]) || fields[1].replace(/^\*/, '') !== filename) {
    throw new Error(`invalid SHA-256 sidecar for ${filename}`);
  }
  if (createHash('sha256').update(bytes).digest('hex') !== fields[0].toLowerCase()) {
    throw new Error(`SHA-256 mismatch for ${filename}`);
  }
}

async function download(url, limit, fetchImpl) {
  const response = await fetchImpl(url, { signal: AbortSignal.timeout(120_000) });
  if (!response.ok) throw new Error(`download failed (${response.status}): ${url}`);
  const chunks = [];
  let size = 0;
  for await (const chunk of response.body) {
    size += chunk.length;
    if (size > limit) throw new Error(`download exceeds size limit: ${url}`);
    chunks.push(Buffer.from(chunk));
  }
  return Buffer.concat(chunks);
}

function runChecked(command, args, options) {
  const result = spawnSync(command, args, options);
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed: ${result.stderr || result.signal}`);
  // asm198x reports --version on stderr (its summary channel). Read both
  // text channels; binary extraction must read stdout alone.
  return options.encoding === 'utf8' ? result.stdout + result.stderr : result.stdout;
}

async function install({ env = process.env, platform = process.platform, arch = process.arch, fetchImpl = fetch, execute = runChecked } = {}) {
  const version = versionOf(env.INPUT_VERSION);
  const asset = releaseAsset(platform, arch);
  for (const key of ['RUNNER_TEMP', 'GITHUB_PATH', 'GITHUB_OUTPUT']) {
    if (!env[key] || /[\r\n]/.test(env[key])) throw new Error(`missing or invalid ${key}`);
  }
  const base = `https://github.com/asm198x/asm198x/releases/download/asm198x-v${version}`;
  const [archive, checksum] = await Promise.all([
    download(`${base}/${asset.archive}`, 128 * 1024 * 1024, fetchImpl),
    download(`${base}/${asset.archive}.sha256`, 4096, fetchImpl),
  ]);
  verifyChecksum(archive, checksum.toString('utf8'), asset.archive);
  const directory = await fs.mkdtemp(path.join(env.RUNNER_TEMP, `asm198x-${version}-`));
  try {
    const archivePath = path.join(directory, asset.archive);
    await fs.writeFile(archivePath, archive);
    const bin = path.join(directory, 'bin');
    await fs.mkdir(bin);
    // Extract only the expected member to stdout. Archive filenames, links
    // and permissions never control a filesystem write.
    const bytes = execute('tar', ['-xOf', archivePath, asset.member], { maxBuffer: 128 * 1024 * 1024 });
    if (bytes.length === 0) throw new Error('release archive contains an empty executable');
    const executable = path.join(bin, asset.binary);
    await fs.writeFile(executable, bytes, { mode: 0o755 });
    const actual = execute(executable, ['--version'], { encoding: 'utf8', timeout: 30_000 }).trim();
    if (actual !== `asm198x v${version}`) throw new Error(`release version mismatch: ${actual}`);
    // Only a verified, runnable binary reaches PATH. No shell interpolation
    // of the version, paths or downloaded content is involved.
    await fs.unlink(archivePath);
    await fs.appendFile(env.GITHUB_OUTPUT, `version=${version}\ntarget=${asset.target}\npath=${bin}\n`);
    await fs.appendFile(env.GITHUB_PATH, `${bin}\n`);
    return { version, target: asset.target, path: bin };
  } catch (error) {
    await fs.rm(directory, { recursive: true, force: true });
    throw error;
  }
}

module.exports = { versionOf, releaseAsset, verifyChecksum, runChecked, install };

if (require.main === module) {
  install().then(result => {
    console.log(`Installed asm198x v${result.version} (${result.target})`);
  }).catch(error => {
    // Plain stderr, not a workflow command containing untrusted message text.
    console.error(`asm198x setup failed: ${error.message}`);
    process.exitCode = 1;
  });
}
