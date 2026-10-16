'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const {createHash} = require('node:crypto');
// Freeze the 0.0.71 evidence contract; later gate additions must not invalidate it.
const targets = ['aarch64-apple-darwin', 'x86_64-apple-darwin', 'x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu', 'aarch64-unknown-linux-gnu'];
const requiredCases = ["archive_identity", "version", "reference_bytes", "determinism", "structured_success", "structured_failure", "preserve_previous_outputs", "missing_input_json", "unknown_cpu", "incompatible_cpu", "hardlink_source", "hardlink_sidecar", "formatter_container", "formatter_origin", "lua", "upstream_project", "disasm_options", "disasm_wide_origin", "pasmo_nested_repeat", "quoted_data_native", "quoted_data_format", "conversion_quoted_data", "conversion_file_scope", "sjasmplus_decoded_escapes", "pasmo_word_arithmetic", "ca65_configured_extent", "ca65_extent_refusal", "ca65_absent_segments", "pasmo_wrapped_image", "sjasmplus_word_operators", "ca65_used_extents", "sjasmplus_logical_expressions", "sjasmplus_shift_widths"];
function qualifies(results) {
  return results.length === 33 && new Set(results.map(r => r.name)).size === 33 && requiredCases.every(name => results.some(r => r.name === name && r.status === 'pass'));
}
function nativeTarget(platform, arch) {
  return {'darwin-arm64': targets[0], 'darwin-x64': targets[1], 'win32-x64': targets[2], 'linux-x64': targets[3], 'linux-arm64': targets[4]}[`${platform}-${arch}`];
}
function packageIdentity(plan) {
  assert.equal(plan.releases.length, 1); const release = plan.releases[0];
  assert.equal(release.app_name, 'asm198x'); assert.equal(release.app_version, '0.0.71');
  for (const target of targets) {
    const name = `asm198x-${target}.${target.includes('windows') ? 'zip' : 'tar.xz'}`;
    assert(release.artifacts.includes(name)); assert.deepEqual(plan.artifacts[name].target_triples, [target]);
    assert.equal(plan.artifacts[name].checksum, name + '.sha256');
  }
  return {version: release.app_version};
}
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
function verify(root) {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json')));
  assert.equal(manifest.version, '0.0.71');
  assert.equal(manifest.tag, 'asm198x-v0.0.71');
  assert.match(manifest.release_commit, /^[a-f0-9]{40}$/);
  assert.match(manifest.cargo_lock_sha256, /^[a-f0-9]{64}$/);
  const expected = ['verify.cjs', 'verify-controls.cjs', 'report.md', 'plan.md', 'premerge.json', 'fix-premerge.json', 'release-timeline.json', 'published-timeline.json', 'release.json', 'older-public-control.json', 'followups.json', 'release-api.json'];
  for (const phase of ['candidate', 'published']) for (const target of targets) {
    for (const name of ['report.json', 'plan.json', 'acquisition.json', 'wrong-bytes.json', 'corrupt-archive.json']) expected.push(`${phase}/${target}/${name}`);
  }
  assert.deepEqual(Object.keys(manifest.files).sort(), expected.sort());
  for (const [name, checksum] of Object.entries(manifest.files)) {
    assert.equal(sha(fs.readFileSync(path.join(root, name))), checksum, `retained checksum ${name}`);
  }
  const fixes = JSON.parse(fs.readFileSync(path.join(root, 'fix-premerge.json')));
  assert.deepEqual(fixes.map(f => f.head).sort(), ['9af7d47a4f627eed70333ac1621d6ce5fc0de25d', '1fa182f226c36deb013dd87c06725e3a503b1a77', '9a3f994e97a3e0131aeb86ea42c3fb12d4b962f7', 'bfde8d8f7f763de385dd3c65a42fe613820e22ca'].sort());
  for (const fix of fixes) {
    assert.equal(fix.checks.length, 33);
    assert(fix.checks.every(c => c.status === 'completed' && ['success', 'skipped', 'neutral'].includes(c.conclusion)));
    assert.equal(fix.runs.length, 5);
    assert(fix.runs.every(r => r.head_sha === fix.head && r.status === 'completed' && ['success', 'skipped', 'neutral'].includes(r.conclusion)));
  }
  assert.match(manifest.pr_head, /^[a-f0-9]{40}$/);
  const premerge = JSON.parse(fs.readFileSync(path.join(root, 'premerge.json')));
  assert.equal(premerge.head, manifest.pr_head);
  assert.equal(premerge.checks.length, 38);
  assert(premerge.checks.every(c => c.status === 'completed' && ['success', 'skipped', 'neutral'].includes(c.conclusion)));
  assert.equal(premerge.runs.length, 7);
  assert(premerge.runs.every(r => r.head_sha === premerge.head && r.status === 'completed' && ['success', 'skipped', 'neutral'].includes(r.conclusion)));
  const timeline = JSON.parse(fs.readFileSync(path.join(root, 'release-timeline.json')));
  assert.equal(timeline.databaseId.toString(), manifest.candidate_run);
  assert.equal(timeline.headSha, manifest.release_commit);
  assert.equal(timeline.status, 'completed'); assert.equal(timeline.conclusion, 'success');
  const qualification = timeline.jobs.filter(j => j.name.includes('Qualify archive ('));
  assert.equal(qualification.length, targets.length);
  for (const job of qualification) assert.equal(job.conclusion, 'success');
  const host = timeline.jobs.find(j => j.name === 'host');
  assert(host && host.conclusion === 'success');
  assert(Date.parse(host.startedAt) >= Math.max(...qualification.map(j => Date.parse(j.completedAt))), 'hosting preceded qualification');
  const publishedTimeline = JSON.parse(fs.readFileSync(path.join(root, 'published-timeline.json')));
  assert.equal(publishedTimeline.databaseId.toString(), manifest.published_run);
  assert.equal(publishedTimeline.status, 'completed'); assert.equal(publishedTimeline.conclusion, 'success');
  const publishedJobs = publishedTimeline.jobs.filter(j => j.name.startsWith('Qualify archive ('));
  assert.equal(publishedJobs.length, targets.length);
  for (const job of publishedJobs) assert.equal(job.conclusion, 'success');
  const older = JSON.parse(fs.readFileSync(path.join(root, 'older-public-control.json')));
  assert.equal(older.version, '0.0.70'); assert.equal(older.target, 'aarch64-apple-darwin');
  assert.equal(older.archive_sha256, 'fc5115ffdf80ca5500c40c48038aba0b7341f5e9e6db4f04cb1a43200cb68f52');
  assert.equal(older.executable_sha256, 'e42862a66480883f5457a93181cbb0b156d13c5bef1851ef0a3de367bcc909d6');
  assert.equal(older.source_lock_scope, 'verified-producer-lock');
  assert.equal(older.source_lock.source_commit, '4d778cd2b424ff2b7b2e93c03e4096ec79e1ccf0');
  assert.equal(older.source_lock.committed_lock_sha256, 'db159ba5cfbe63c0c21cbeb00e8629c695daa999d423a9e3ab08792e0058730d');
  assert.equal(older.qualified, false); assert.equal(older.results.length, 33);
  assert.deepEqual(older.results.map(x => x.name).sort(), [...requiredCases].sort());
  assert.deepEqual(older.results.filter(x => x.status !== 'pass').map(x => x.name).sort(), ['ca65_used_extents', 'sjasmplus_logical_expressions', 'sjasmplus_shift_widths']);
  const release = JSON.parse(fs.readFileSync(path.join(root, 'release.json')));
  const api = JSON.parse(fs.readFileSync(path.join(root, 'release-api.json')));
  assert.equal(api.tag_name, release.tagName); assert.equal(api.published_at, release.publishedAt);
  assert.deepEqual(api.assets, release.assets);
  assert.equal(release.tagName, manifest.tag); assert.equal(release.isDraft, false);
  assert.equal(release.url, `https://github.com/asm198x/asm198x/releases/tag/${manifest.tag}`);
  assert(Date.parse(release.publishedAt) >= Math.max(...qualification.map(j => Date.parse(j.completedAt))));
  const records = {};
  for (const phase of ['candidate', 'published']) {
    records[phase] = {};
    for (const target of targets) {
      const read = name => JSON.parse(fs.readFileSync(path.join(root, phase, target, name)));
      const r = read('report.json');
      assert.equal(r.target, target); assert.equal(r.version, manifest.version);
      assert.equal(r.qualified, true); assert(qualifies(r.results), `${phase}/${target} qualification`);
      assert.equal(nativeTarget(r.host.platform, r.host.arch), target);
      assert.equal(r.host.github_run, manifest[`${phase}_run`]);
      assert.equal(r.checksum_mutation_rejected, true);
      assert.equal(r.source_lock_scope, 'verified-producer-lock');
      const receipt = r.source_lock;
      assert(receipt, 'missing producer receipt');
      assert.equal(receipt.schema_version, 1); assert.equal(receipt.kind, 'producer-source-lock');
      assert.equal(receipt.signed, false); assert.equal(receipt.compiled_dependency_graph, 'unknown');
      assert.equal(receipt.source_commit, manifest.release_commit);
      for (const key of ['committed_lock_sha256', 'before_lock_sha256', 'after_lock_sha256']) assert.equal(receipt[key], manifest.cargo_lock_sha256);
      for (const key of ['target', 'archive', 'archive_sha256', 'executable_sha256', 'version']) assert.equal(receipt[key], r[key]);
      assert.equal(receipt.build.run_id, manifest.candidate_run);
      assert.equal(receipt.build.run_attempt, String(timeline.attempt));
      assert(receipt.build.job && receipt.command.includes('dist') && receipt.command.includes('build') && receipt.command.some(arg => arg === target || arg === `--target=${target}`));
      for (const name of ['archive_sha256', 'executable_sha256']) assert.match(r[name], /^[a-f0-9]{64}$/);
      const plan = read('plan.json');
      assert.equal(packageIdentity(plan, target).version, manifest.version);
      assert.equal(r.parsed_plan_sha256, sha(Buffer.from(JSON.stringify(plan))));
      if (phase === 'published') {
        const asset = release.assets.find(a => a.name === r.archive);
        assert(asset && asset.state === 'uploaded'); assert.equal(asset.digest, `sha256:${r.archive_sha256}`);
        const receiptAsset = release.assets.find(a => a.name === `asm198x-${target}-source-lock.json`);
        assert(receiptAsset && receiptAsset.state === 'uploaded');
        assert.equal(receiptAsset.digest, `sha256:${sha(Buffer.from(JSON.stringify(receipt, null, 2) + '\n'))}`);
      }
      const acquisition = read('acquisition.json');
      assert.equal(acquisition.run, manifest[`${phase}_run`]);
      assert.equal(acquisition.checkout_commit, r.checkout_commit);
      assert.equal(acquisition.kind, phase === 'candidate' ? 'candidate' : 'published-release');
      if (phase === 'candidate') assert.equal(r.checkout_commit, manifest.release_commit);
      else { assert.equal(acquisition.published_version, manifest.version); assert.equal(r.checkout_commit, publishedTimeline.headSha); }
      const wrong = read('wrong-bytes.json');
      assert.equal(wrong.qualified, false);
      assert.deepEqual(wrong.results.map(x => x.name).sort(), [...requiredCases].sort());
      assert(wrong.results.every(x => ['pass', 'fail'].includes(x.status)));
      assert.equal(wrong.source_lock_scope, 'verified-producer-lock');
      assert.deepEqual(wrong.source_lock, receipt);
      assert.equal(wrong.target, target); assert.equal(wrong.archive_sha256, r.archive_sha256);
      assert.equal(wrong.executable_sha256, r.executable_sha256);
      assert.deepEqual(wrong.results.filter(x => x.status !== 'pass').map(x => x.name).sort(), ['reference_bytes', 'determinism', 'structured_success'].sort());
      const corrupt = read('corrupt-archive.json');
      assert.equal(corrupt.qualified, false); assert.equal(corrupt.target, target);
      assert.deepEqual(corrupt.results, []); assert(corrupt.error.includes('SHA-256 mismatch'));
      records[phase][target] = r;
    }
  }
  for (const target of targets) assert.deepEqual(records.candidate[target].source_lock, records.published[target].source_lock, `published receipt ${target}`);
  for (const target of targets) for (const key of ['archive_sha256', 'executable_sha256', 'version', 'target']) {
    assert.equal(records.candidate[target][key], records.published[target][key], `publication identity ${target}/${key}`);
  }
  console.log('Verified five native candidate/public pairs, 330 passing cases, 20 rejected corruption controls and retained-file hashes.');
}
module.exports = {verify};
if (require.main === module) verify(path.resolve(process.argv[2] || __dirname));
