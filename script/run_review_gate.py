#!/usr/bin/env python3
"""Run all standalone offline QA and performance modes from a frozen candidate.

Crash-parent integration tests and native interaction/performance acceptance
remain separate required gates; exit zero here covers only this ledger.

`--qa-automation-perf` is retained as a non-gating legacy attachment-latency
diagnostic: its single raw save-block sample is recorded for comparison, but
only its correctness exit code gates. The required production responsiveness
gate is `--qa-live-performance`, which must run the 500- and 1,000-occurrence
fixtures and keep every per-operation p95 below 50 ms. Missing live entry,
fixture or gate coverage fails this ledger closed.
"""
import argparse, hashlib, json, math, os, pathlib, re, shutil, sqlite3, subprocess, time

AUTOMATION_PERF_ENTRY = '--qa-automation-perf'
AUTOMATION_PERF_PERIODS = 1000
LIVE_ENTRY = '--qa-live-performance'
LIVE_TARGET_UI_US = 50_000
LIVE_REQUIRED_FIXTURES = ('small-500', 'required-1000')
LIVE_REQUIRED_OCCURRENCES = {'small-500': 500, 'required-1000': 1000}
LIVE_REQUIRED_OPERATIONS = (
    'editor',
    'automation_save_draft',
    'automation_enable',
    'automation_pause',
    'settings',
    'scoped_task_undo',
    'history_page',
)


def json_report(text):
    """Decode the first JSON object in a log that may have prefix/suffix lines."""
    start = text.find('{')
    if start < 0:
        return None
    try:
        value, _ = json.JSONDecoder().raw_decode(text[start:])
    except ValueError:
        return None
    return value if isinstance(value, dict) else None


def classify_automation_perf(code, log_text):
    """Separate functional correctness from the legacy latency observation."""
    measured = re.search(r'synchronous UI-triggered save block (\d+) ms', log_text)
    fixture_match = re.search(r'performance fixture: ([\d,]+) occurrences', log_text)
    measured_periods = int(fixture_match.group(1).replace(',', '')) if fixture_match else None
    diagnostic = {
        'kind': 'legacy_attachment_latency',
        'gating': False,
        'sample_count': 1,
        'expected_periods': AUTOMATION_PERF_PERIODS,
        'period_scope_matches': measured_periods == AUTOMATION_PERF_PERIODS,
        'acceptance_note': (
            'One synchronous whole-store save-block sample on a legacy QA attachment; '
            'it is not p95, not native scrolling and not the production performance gate. '
            'Required live performance is --qa-live-performance.'
        ),
    }
    result = {
        'correctness': 'passed' if code == 0 else 'failed',
        'status': 'diagnostic' if code == 0 else 'failed',
        'diagnostic': diagnostic,
    }
    if measured:
        milliseconds = int(measured.group(1))
        result['ui_save_block_ms'] = milliseconds
        diagnostic['ui_save_block_ms'] = milliseconds
        diagnostic['measurement'] = 'available'
    else:
        diagnostic['measurement'] = 'unavailable'
    if measured_periods is not None:
        result['fixture_periods'] = measured_periods
        diagnostic['fixture_periods'] = measured_periods
    if code != 0:
        diagnostic['acceptance_note'] = (
            'The process exited nonzero; that correctness failure gates. '
            + diagnostic['acceptance_note']
        )
    return result


def finite_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def classify_live_performance(code, log_text):
    """Require the production 500/1,000 coverage and every numeric 50 ms p95 gate."""
    report = json_report(log_text)
    problems = []
    target = report.get('target_p95_ui_us') if report else None
    fixtures = {}
    gates = {}
    if report is None:
        problems.append('no JSON report')
    else:
        fixtures = {fixture.get('label'): fixture for fixture in report.get('fixtures', []) if isinstance(fixture, dict)}
        gates = report.get('gates') if isinstance(report.get('gates'), dict) else {}
        if not finite_number(target) or target != LIVE_TARGET_UI_US:
            problems.append(f'target_p95_ui_us is {target!r}, expected numeric {LIVE_TARGET_UI_US}')
        for label in LIVE_REQUIRED_FIXTURES:
            expected_occurrences = LIVE_REQUIRED_OCCURRENCES[label]
            fixture = fixtures.get(label)
            if not fixture or fixture.get('gating') is not True:
                problems.append(f'{label} is missing or not gating')
            elif not finite_number(fixture.get('occurrences')) or fixture.get('occurrences') != expected_occurrences:
                problems.append(
                    f'{label} occurrences is {fixture.get("occurrences")!r}, expected {expected_occurrences}'
                )
            fixture_gates = gates.get(label)
            if not isinstance(fixture_gates, dict) or not fixture_gates:
                problems.append(f'{label} has no per-operation gates')
                continue
            for operation in LIVE_REQUIRED_OPERATIONS:
                if operation not in fixture_gates:
                    problems.append(f'{label}/{operation} gate is missing')
            for operation, gate in fixture_gates.items():
                if not isinstance(gate, dict):
                    problems.append(f'{label}/{operation} gate is malformed')
                    continue
                gate_target = gate.get('target_us')
                if not finite_number(gate_target) or gate_target != LIVE_TARGET_UI_US:
                    problems.append(f'{label}/{operation} target is {gate_target!r}')
                p95 = gate.get('p95_us')
                if not finite_number(p95) or p95 < 0:
                    problems.append(f'{label}/{operation} p95_us is not a finite nonnegative number: {p95!r}')
                elif p95 >= LIVE_TARGET_UI_US:
                    problems.append(f'{label}/{operation} p95_us {p95} did not beat {LIVE_TARGET_UI_US}')
                elif gate.get('passed') is not True:
                    problems.append(f'{label}/{operation} did not pass')
    if code != 0:
        problems.insert(0, f'process exit {code}')
    observation = {
        'gating': True,
        'target_p95_ui_us': target,
        'required_fixtures': list(LIVE_REQUIRED_FIXTURES),
        'required_occurrences': dict(LIVE_REQUIRED_OCCURRENCES),
        'required_operations': list(LIVE_REQUIRED_OPERATIONS),
        'coverage_ok': not problems,
        'acceptance_note': (
            'Required live-backend production responsiveness gate: 500- and 1,000-occurrence '
            'fixtures, finite nonnegative per-operation p95_us strictly below 50,000. '
            'Missing or nonnumeric coverage fails closed.'
        ),
    }
    if report is not None:
        observation['fixtures'] = [
            {'label': fixture.get('label'), 'gating': fixture.get('gating'), 'occurrences': fixture.get('occurrences')}
            for fixture in fixtures.values()
        ]
    result = {
        'status': 'passed' if not problems else 'failed',
        'correctness': 'passed' if code == 0 else 'failed',
        'live_performance': observation,
    }
    if problems:
        result['coverage_error'] = '; '.join(problems[:12])
    return result


def gate_failures(results):
    failures = []
    seen_live = False
    for result in results:
        entry = result['entry']
        if entry == AUTOMATION_PERF_ENTRY:
            if result.get('correctness') != 'passed':
                failures.append(entry)
        elif entry == LIVE_ENTRY:
            seen_live = True
            if result.get('status') != 'passed':
                failures.append(entry)
        elif result.get('status') != 'passed':
            failures.append(entry)
    if not seen_live:
        failures.append(LIVE_ENTRY)
    return failures


def gate_summary(results):
    failures = gate_failures(results)
    diagnostics = [result['entry'] for result in results if result.get('status') == 'diagnostic']
    required = [result for result in results if result.get('status') != 'diagnostic']
    return {
        'overall': 'failed' if failures else ('passed_with_non_gating_diagnostics' if diagnostics else 'passed'),
        'required_entries': len(required),
        'required_passed': sum(1 for result in required if result.get('status') == 'passed'),
        'non_gating_diagnostics': diagnostics,
        'gating_failures': failures,
        'automation_perf_latency_gating': False,
        'live_performance_gating': True,
    }


def live_gate_problems(flags, live_source):
    problems = []
    if LIVE_ENTRY not in flags:
        problems.append(f'{LIVE_ENTRY} entry is missing from src/main.rs')
    if 'TARGET_P95_UI_US: u64 = 50_000' not in live_source:
        problems.append('live performance 50 ms target is missing')
    for token in ('("small-500", small, true)', '("required-1000", required, true)'):
        if token not in live_source:
            problems.append(f'required live fixture is missing: {token}')
    return problems


def self_test():
    high = classify_automation_perf(
        0,
        'performance fixture: 1,000 occurrences (20 with full research entities)\n'
        'performance fixture UI: synchronous UI-triggered save block 1342 ms',
    )
    assert high['status'] == 'diagnostic' and high['correctness'] == 'passed'
    assert high['ui_save_block_ms'] == 1342 and high['diagnostic']['gating'] is False
    assert AUTOMATION_PERF_ENTRY not in gate_failures([{'entry': AUTOMATION_PERF_ENTRY, **high}, {'entry': LIVE_ENTRY, 'status': 'passed'}])
    functional = classify_automation_perf(101, 'performance fixture: 1,000 occurrences')
    assert functional['status'] == 'failed' and functional['correctness'] == 'failed'
    assert gate_failures([{'entry': AUTOMATION_PERF_ENTRY, **functional}, {'entry': LIVE_ENTRY, 'status': 'passed'}])
    unavailable = classify_automation_perf(0, 'performance fixture: 1,000 occurrences')
    assert unavailable['status'] == 'diagnostic'
    assert unavailable['diagnostic']['measurement'] == 'unavailable'
    wrong_scale = classify_automation_perf(0, 'performance fixture: 43,200 occurrences\nsynchronous UI-triggered save block 75 ms')
    assert wrong_scale['status'] == 'diagnostic' and not wrong_scale['diagnostic']['period_scope_matches']

    def gate(p95_us):
        return {'target_us': LIVE_TARGET_UI_US, 'p95_us': p95_us, 'passed': True}

    gates = {
        label: {name: gate(8000 + index) for index, name in enumerate(LIVE_REQUIRED_OPERATIONS)}
        for label in LIVE_REQUIRED_FIXTURES
    }
    fixtures = [
        {'label': 'small-500', 'gating': True, 'occurrences': 500},
        {'label': 'required-1000', 'gating': True, 'occurrences': 1000},
    ]
    complete = {'target_p95_ui_us': LIVE_TARGET_UI_US, 'fixtures': fixtures, 'gates': gates}
    live_ok = classify_live_performance(0, json.dumps(complete) + '\nLIVE_PERFORMANCE_QA status=ok')
    assert live_ok['status'] == 'passed' and live_ok['live_performance']['coverage_ok']
    assert not gate_failures([{'entry': LIVE_ENTRY, **live_ok}])

    missing = dict(complete)
    missing['fixtures'] = fixtures[:1]
    missing['gates'] = {'small-500': gates['small-500']}
    live_missing = classify_live_performance(0, json.dumps(missing))
    assert live_missing['status'] == 'failed' and 'required-1000' in live_missing['coverage_error']
    assert gate_failures([{'entry': LIVE_ENTRY, **live_missing}])

    rejected = json.loads(json.dumps(complete))
    rejected['gates']['required-1000']['history_page']['passed'] = False
    live_rejected = classify_live_performance(0, json.dumps(rejected))
    assert live_rejected['status'] == 'failed'
    assert 'required-1000/history_page did not pass' in live_rejected['coverage_error']

    slow = json.loads(json.dumps(complete))
    slow['gates']['required-1000']['editor']['p95_us'] = LIVE_TARGET_UI_US
    live_slow = classify_live_performance(0, json.dumps(slow))
    assert live_slow['status'] == 'failed' and 'did not beat 50000' in live_slow['coverage_error']

    boolean = json.loads(json.dumps(complete))
    boolean['gates']['small-500']['settings']['p95_us'] = True
    live_boolean = classify_live_performance(0, json.dumps(boolean))
    assert live_boolean['status'] == 'failed' and 'finite nonnegative number' in live_boolean['coverage_error']

    negative = json.loads(json.dumps(complete))
    negative['gates']['small-500']['settings']['p95_us'] = -1
    live_negative = classify_live_performance(0, json.dumps(negative))
    assert live_negative['status'] == 'failed' and 'finite nonnegative number' in live_negative['coverage_error']

    wrong_occurrences = json.loads(json.dumps(complete))
    wrong_occurrences['fixtures'][1]['occurrences'] = 43200
    live_wrong_occurrences = classify_live_performance(0, json.dumps(wrong_occurrences))
    assert live_wrong_occurrences['status'] == 'failed' and 'occurrences is 43200' in live_wrong_occurrences['coverage_error']

    live_exit = classify_live_performance(1, json.dumps(complete))
    assert live_exit['status'] == 'failed' and 'process exit 1' in live_exit['coverage_error']
    assert gate_failures([{'entry': AUTOMATION_PERF_ENTRY, **high}]) == [LIVE_ENTRY]

    assert live_gate_problems({LIVE_ENTRY}, 'TARGET_P95_UI_US: u64 = 50_000\n("small-500", small, true)\n("required-1000", required, true)') == []
    assert live_gate_problems(set(), '')

    diagnostic_summary = gate_summary([{'entry': AUTOMATION_PERF_ENTRY, **high}, {'entry': LIVE_ENTRY, **live_ok}])
    assert diagnostic_summary['overall'] == 'passed_with_non_gating_diagnostics'
    assert diagnostic_summary['non_gating_diagnostics'] == [AUTOMATION_PERF_ENTRY]
    assert diagnostic_summary['required_passed'] == 1 and diagnostic_summary['gating_failures'] == []
    assert gate_summary([{'entry': LIVE_ENTRY, **live_ok}])['overall'] == 'passed'
    assert gate_summary([{'entry': AUTOMATION_PERF_ENTRY, **functional}, {'entry': LIVE_ENTRY, **live_ok}])['overall'] == 'failed'
    missing_live_summary = gate_summary([{'entry': AUTOMATION_PERF_ENTRY, **high}])
    assert missing_live_summary['overall'] == 'failed' and missing_live_summary['gating_failures'] == [LIVE_ENTRY]
    print('RUNNER_SELF_TEST status=ok cases=13')


parser = argparse.ArgumentParser()
parser.add_argument('--self-test', action='store_true', help='Run in-memory classification checks only')
parser.add_argument('--output')
parser.add_argument('--source-root', help='Frozen source used to build the reviewed candidate')
parser.add_argument('--binary', help='Reviewed immutable QA-enabled candidate')
parser.add_argument('--expected-sha256', help='SHA-256 from the reviewed candidate build manifest')
parser.add_argument('--fixture-db')
args = parser.parse_args()
if args.self_test:
    self_test()
    raise SystemExit(0)
if not all((args.output, args.source_root, args.binary, args.expected_sha256, args.fixture_db)):
    parser.error('--output, --source-root, --binary, --expected-sha256 and --fixture-db are required')
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
live_source_path = source_root/'src/live_performance_qa.rs'
if not live_source_path.is_file():
    raise SystemExit('Required live performance source is missing; gate aborted')
live_problems = live_gate_problems(flags, live_source_path.read_text())
if live_problems:
    raise SystemExit('Required live performance gate is missing or weakened: ' + '; '.join(live_problems))
entries = [{'entry': flag, 'argv': [flag]} for flag in sorted(flags - set(crash_children))]
assert any(entry['entry'] == LIVE_ENTRY for entry in entries), 'Required live performance entry is not scheduled'
perf_source = (source_root/'src/persistence_qa.rs').read_text()
perf_modes = ['inventory', 'small', 'occ43200', 'body-heavy', 'retry-heavy', 'xl']
assert all('\"' + mode + '\"' + ' =>' in perf_source for mode in perf_modes), 'Performance modes changed; reconcile coverage'
entries += [{'entry': '--qa-persistence-perf ' + mode, 'argv': ['--qa-persistence-perf', mode]} for mode in perf_modes]

manifest = {
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'guard_exit': guard.returncode,
    'source_hash_note': 'Frozen-source inventory; reviewed build manifest is still required to prove binary/source provenance.',
    'frozen_source_sha256': {str(p.relative_to(source_root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((source_root/'src').glob('*.rs'))},
    'source_root': str(source_root),
    'required_separate_crash_tests': crash_children,
    'scope': 'All standalone offline QA and performance modes; parent crash tests and native matrix require separate evidence.',
    'automation_perf_scope': {
        'classification': 'legacy attachment-latency diagnostic',
        'required_periods': AUTOMATION_PERF_PERIODS,
        'required_env': 'PLANNING_QA_PERIODS=1000',
        'gating': 'Only the correctness exit code gates; the single raw ui_save_block_ms sample is non-gating.',
        'acceptance_note': 'Not p95, not native scrolling and not the production performance gate; required live performance is --qa-live-performance.',
        'stress_modes': 'PLANNING_QA_PERIODS=43200/86400 are explicit non-gating stress diagnostics and are not run by this gate.',
    },
    'live_performance_scope': {
        'entry': LIVE_ENTRY,
        'required_fixtures': list(LIVE_REQUIRED_FIXTURES),
        'target_p95_ui_us': LIVE_TARGET_UI_US,
        'gating': 'Missing entry/source, required fixture, required operation gate or any p95 >= 50 ms fails this ledger closed.',
        'native_note': 'This measures the live TaskApp/save/history path; native scrolling acceptance remains separate.',
    },
    'results': [],
}
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
    run_env = dict(env)
    if flag == AUTOMATION_PERF_ENTRY:
        run_env['PLANNING_QA_PERIODS'] = str(AUTOMATION_PERF_PERIODS)
    print('START', flag, flush=True)
    with log.open('w') as stream:
        try:
            result = subprocess.run([str(binary), *entry['argv']], env=run_env, stdout=stream, stderr=subprocess.STDOUT, timeout=1200)
            code = result.returncode
            status = 'passed' if code == 0 else 'failed'
        except subprocess.TimeoutExpired:
            code, status = None, 'timeout'
            stream.write('\n[timeout] Independent gate exceeded 1200 seconds. Diagnose before changing bounds.\n')
    observation = {}
    log_text = log.read_text() if flag in (AUTOMATION_PERF_ENTRY, LIVE_ENTRY) else ''
    if flag == AUTOMATION_PERF_ENTRY:
        observation = classify_automation_perf(code, log_text)
        if code is None:
            observation['status'] = 'timeout'
            observation['correctness'] = 'failed'
    elif flag == LIVE_ENTRY:
        observation = classify_live_performance(code, log_text)
        if code is None:
            observation['status'] = 'timeout'
            observation['correctness'] = 'failed'
    manifest['results'].append({
        'entry': flag,
        'status': observation.get('status', status),
        'exit_code': code,
        'elapsed_seconds': round(time.time()-started, 2),
        'log': log.name,
        'alias_note': entry.get('note', ''),
        **observation,
    })
    (out/'index.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print('END', flag, manifest['results'][-1]['status'], code, flush=True)

summary = gate_summary(manifest['results'])
failures = summary['gating_failures']
manifest['summary'] = summary
(out/'index.json').write_text(json.dumps(manifest, indent=2)+'\n')
print(
    'GATE_SUMMARY overall={} required_passed={}/{} diagnostics={} gating_failures={}'.format(
        manifest['summary']['overall'],
        manifest['summary']['required_passed'],
        manifest['summary']['required_entries'],
        summary['non_gating_diagnostics'] or 'none',
        failures or 'none',
    ),
    flush=True,
)
raise SystemExit(1 if failures else 0)
