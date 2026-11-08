#!/usr/bin/env python3
"""Run all standalone offline QA and performance modes from a frozen candidate.

Crash-parent integration tests and native interaction/performance acceptance
remain separate required gates; exit zero here covers only this ledger."""
import argparse, hashlib, json, os, pathlib, re, shutil, sqlite3, subprocess, time
parser = argparse.ArgumentParser()
parser.add_argument('--output', required=True)
parser.add_argument('--source-root', required=True, help='Frozen source used to build the reviewed candidate')
parser.add_argument('--binary', required=True, help='Reviewed immutable QA-enabled candidate')
parser.add_argument('--expected-sha256', required=True, help='SHA-256 from the reviewed candidate build manifest')
parser.add_argument('--fixture-db', required=True)
args = parser.parse_args()
root = pathlib.Path.cwd()
out = pathlib.Path(args.output).resolve()
out.mkdir(parents=True, exist_ok=False)
built_binary = pathlib.Path(args.binary).resolve()
expected_hash = args.expected_sha256.lower()
if len(expected_hash) != 64 or any(c not in '0123456789abcdef' for c in expected_hash):
    raise SystemExit('Invalid expected SHA-256')
if hashlib.sha256(built_binary.read_bytes()).hexdigest() != expected_hash:
    raise SystemExit('Candidate hash does not match the reviewed build; no command executed')
binary = out / 'qa-candidate'
shutil.copy2(built_binary, binary)
if hashlib.sha256(binary.read_bytes()).hexdigest() != expected_hash:
    raise SystemExit('Candidate changed while copying; no command executed')
# The reviewed source must implement this fail-closed guard before startup.
guard = subprocess.run([str(binary), '--qa-independent-invalid-guard'], capture_output=True, text=True, timeout=15)
(out / 'qa-guard.log').write_text(guard.stdout + guard.stderr)
if guard.returncode != 2 or 'No workspace was opened.' not in guard.stderr:
    raise SystemExit('Candidate failed the startup guard; release gate aborted')
source_root = pathlib.Path(args.source_root).resolve()
main_source = (source_root/'src/main.rs').read_text()
flags = set(re.findall(r'arg == "(--qa[^" ]*)"', main_source))
historical = json.loads((root/'doc/review-2026-11-08/evidence/qa-entrypoint-index.json').read_text())
assert {entry['entry'] for entry in historical} <= flags, 'Historical coverage disappeared; review explicitly'
crash_children = {'--qa-dispatch-crash': 'dispatch_admission_crash', '--qa-v4-crash': 'v4_crash'}
assert set(crash_children) <= flags
entries = [{'entry': flag, 'argv': [flag]} for flag in sorted(flags - set(crash_children))]
perf_source = (source_root/'src/persistence_qa.rs').read_text()
perf_modes = ['inventory', 'small', 'occ43200', 'body-heavy', 'retry-heavy', 'xl']
assert all('\"' + mode + '\" =>' in perf_source for mode in perf_modes), 'Performance modes changed; reconcile coverage'
entries += [{'entry': '--qa-persistence-perf ' + mode, 'argv': ['--qa-persistence-perf', mode]} for mode in perf_modes]

manifest = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'guard_exit': guard.returncode, 'source_hash_note': 'Frozen-source inventory; reviewed build manifest is still required to prove binary/source provenance.', 'frozen_source_sha256': {str(p.relative_to(source_root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((source_root/'src').glob('*.rs'))}, 'source_root': str(source_root), 'required_separate_crash_tests': crash_children, 'scope': 'All standalone offline QA and performance modes; parent crash tests and native matrix require separate evidence.', 'results': []}
fixture = out/'render-fixture.sqlite3'
source = sqlite3.connect('file:'+str(pathlib.Path(args.fixture_db).resolve())+'?mode=ro', uri=True)
with sqlite3.connect(fixture) as destination: source.backup(destination)
source.close()
env = dict(os.environ)
env['PATH'] = '/opt/homebrew/opt/node@24/bin:' + env.get('PATH', '')
env['PLANNING_PERF_WORKSPACE'] = str(fixture)
for entry in entries:
    flag = entry['entry']
    log = out/(flag.lstrip('-')+'.log')
    started = time.time()
    print('START', flag, flush=True)
    with log.open('w') as stream:
        try:
            result = subprocess.run([str(binary), *entry['argv']], env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=1200)
            code = result.returncode
            status = 'passed' if code == 0 else 'failed'
        except subprocess.TimeoutExpired:
            code, status = None, 'timeout'
            stream.write('\n[timeout] Independent gate exceeded 1200 seconds. Diagnose before changing bounds.\n')
    manifest['results'].append({'entry': flag, 'status': status, 'exit_code': code, 'elapsed_seconds': round(time.time()-started, 2), 'log': log.name, 'alias_note': entry.get('note', '')})
    (out/'index.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print('END', flag, status, code, flush=True)
raise SystemExit(0 if all(r['status']=='passed' for r in manifest['results']) else 1)
