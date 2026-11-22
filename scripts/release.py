#!/usr/bin/env python3
"""Create signed Sparkle release assets, or publish a complete GitHub release."""
import argparse
import base64
import html
import json
import os
from pathlib import Path
import plistlib
import re
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
REPO = 'KrabsWong/what-the-hell-are-you-planning'
FEED_URL = f'https://github.com/{REPO}/releases/latest/download/appcast.xml'
SPARKLE = 'http://www.andymatuschak.org/xml-namespaces/sparkle'
DEFAULT_RELEASE_SUMMARY = 'Improvements and bug fixes.\n'


def run(*args, **kwargs):
    return subprocess.run([str(arg) for arg in args], check=True, **kwargs)


def version_tuple(tag):
    if not re.fullmatch(r'v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)', tag):
        raise ValueError('Release tag must be a stable vMAJOR.MINOR.PATCH version')
    return tuple(int(part) for part in tag[1:].split('.'))


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def cargo_version():
    metadata = json.loads(run('cargo', 'metadata', '--no-deps', '--format-version=1',
                              cwd=ROOT, capture_output=True, text=True).stdout)
    return next(package['version'] for package in metadata['packages']
                if package['name'] == 'what-the-hell-are-you-planning')


def validate_tag(tag):
    version_tuple(tag)
    if tag[1:] != cargo_version():
        raise ValueError('Release tag must match the Cargo package version')
    commit = git('rev-parse', '--verify', f'refs/tags/{tag}^{{commit}}')
    if git('rev-parse', 'HEAD') != commit:
        raise ValueError('Release tag must point at the checked-out commit')
    ancestry = subprocess.run(['git', 'merge-base', '--is-ancestor', commit,
                               'refs/remotes/origin/main'], cwd=ROOT, capture_output=True)
    if ancestry.returncode != 0:
        raise ValueError('Release tag must point to a commit already merged into origin/main')
    return commit


def validate_bundle(info, public_key):
    if not public_key or len(base64.b64decode(public_key, validate=True)) != 32:
        raise ValueError('SPARKLE_PUBLIC_ED_KEY must be a base64 32-byte public key')
    expected = dict(SUFeedURL=FEED_URL, SUPublicEDKey=public_key,
                    SUVerifyUpdateBeforeExtraction=True, SURequireSignedFeed=True,
                    SUEnableAutomaticChecks=True, SUAutomaticallyUpdate=False,
                    SUAllowsAutomaticUpdates=False)
    if any(info.get(key) != value for key, value in expected.items()):
        raise ValueError('Bundle updater configuration or public key does not match release')


def asset_paths(tag):
    return ROOT / f'dist/Planning-{tag[1:]}-macOS-arm64.zip', ROOT / 'dist/appcast.xml'


def load_summary(tag):
    path = ROOT / 'release-notes' / f'{tag}.md'
    try:
        summary = path.read_text(encoding='utf-8').strip()
    except (OSError, UnicodeError):
        return DEFAULT_RELEASE_SUMMARY
    return summary or DEFAULT_RELEASE_SUMMARY


def embedded_release_notes(tag, summary):
    return (f'# What the hell are you planning? {tag[1:]}\n\n{summary}\n'
            f'[View full release notes](https://github.com/{REPO}/releases/tag/{tag})\n')


def prepare(tag, sdk):
    validate_tag(tag)
    app = ROOT / 'dist/Planning.app'
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    if info.get('CFBundleShortVersionString') != tag[1:]:
        raise ValueError('Bundle version does not match the release tag')
    public_key = os.environ.get('SPARKLE_PUBLIC_ED_KEY', '').strip()
    validate_bundle(info, public_key)
    private_key = os.environ.get('SPARKLE_PRIVATE_ED_KEY', '').strip()
    if not private_key or len(base64.b64decode(private_key, validate=True)) != 32:
        raise ValueError('SPARKLE_PRIVATE_ED_KEY must be a base64 32-byte Ed25519 seed')
    swift = '''import Foundation
import CryptoKit
let data = FileHandle.standardInput.readDataToEndOfFile()
let seed = Data(base64Encoded: String(data: data, encoding: .utf8)!.trimmingCharacters(in: .whitespacesAndNewlines))!
let key = try Curve25519.Signing.PrivateKey(rawRepresentation: seed)
print(key.publicKey.rawRepresentation.base64EncodedString())
'''
    actual = run('swift', '-module-cache-path', ROOT / 'build/swift-cache', '-e', swift,
                 input=private_key, capture_output=True, text=True).stdout.strip()
    if actual != public_key:
        raise ValueError('Release signing seed does not match the public key embedded in the app')
    run('codesign', '--verify', '--deep', '--strict', app)
    binary = app / 'Contents/MacOS' / info['CFBundleExecutable']
    if run('lipo', '-archs', binary, capture_output=True, text=True).stdout.strip() != 'arm64':
        raise ValueError('Release executable must be arm64')

    archive, feed = asset_paths(tag)
    archive.unlink(missing_ok=True)
    run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', app, archive)
    signer = sdk / 'bin/sign_update'
    signature = run(signer, '--ed-key-file', '-', '-p', archive,
                    input=private_key, capture_output=True, text=True).stdout.strip()
    if len(base64.b64decode(signature, validate=True)) != 64:
        raise ValueError('Sparkle signer did not return an Ed25519 signature')

    ET.register_namespace('sparkle', SPARKLE)
    root = ET.Element('rss', version='2.0')
    channel = ET.SubElement(root, 'channel')
    ET.SubElement(channel, 'title').text = 'What the hell are you planning? updates'
    item = ET.SubElement(channel, 'item')
    ET.SubElement(item, 'title').text = f'What the hell are you planning? {tag[1:]}'
    ET.SubElement(item, 'link').text = f'https://github.com/{REPO}/releases/tag/{tag}'
    ET.SubElement(item, 'description', {f'{{{SPARKLE}}}format': 'markdown'}).text = \
        embedded_release_notes(tag, load_summary(tag))
    ET.SubElement(item, f'{{{SPARKLE}}}fullReleaseNotesLink').text = \
        f'https://github.com/{REPO}/releases/tag/{tag}'
    ET.SubElement(item, f'{{{SPARKLE}}}version').text = info['CFBundleVersion']
    ET.SubElement(item, f'{{{SPARKLE}}}shortVersionString').text = tag[1:]
    ET.SubElement(item, f'{{{SPARKLE}}}minimumSystemVersion').text = info['LSMinimumSystemVersion']
    ET.SubElement(item, f'{{{SPARKLE}}}hardwareRequirements').text = 'arm64'
    ET.SubElement(item, 'enclosure',
                  url=f'https://github.com/{REPO}/releases/download/{tag}/{archive.name}',
                  length=str(archive.stat().st_size), type='application/octet-stream',
                  attrib={f'{{{SPARKLE}}}edSignature': signature})
    ET.ElementTree(root).write(feed, encoding='utf-8', xml_declaration=True)
    run(signer, '--ed-key-file', '-', feed, input=private_key, capture_output=True, text=True)
    print(f'Prepared {archive.name} and signed appcast.xml')


def github_release(endpoint):
    response = subprocess.run(['gh', 'api', f'repos/{REPO}/releases/{endpoint}'],
                              capture_output=True, text=True)
    if response.returncode:
        if '(HTTP 404)' in response.stderr:
            return None
        raise RuntimeError('Could not inspect GitHub release state; publication stopped')
    return json.loads(response.stdout)


def release_notes(tag, previous):
    commit = validate_tag(tag)
    lines = [f'# What the hell are you planning? {tag[1:]}', '',
             load_summary(tag).rstrip(), '',
             f'Release commit: [{commit}](https://github.com/{REPO}/commit/{commit})', '']
    revision = commit
    if previous:
        previous_commit = git('rev-parse', '--verify', f'refs/tags/{previous}^{{commit}}')
        revision = f'{previous_commit}..{commit}'
        lines += [f'Changes since {previous}.',
                  f'[Full comparison](https://github.com/{REPO}/compare/{previous}...{tag})', '']
    output = run('git', 'log', '--reverse', '--topo-order', '-z', '--format=%H%x00%s', revision,
                 cwd=ROOT, capture_output=True, text=True).stdout
    fields = output.rstrip('\0').split('\0') if output else []
    commits = list(zip(fields[::2], fields[1::2]))
    lines += [f'## Commits ({len(commits)})', '']
    for sha, subject in commits:
        subject = re.sub(r'([\\`*_{}\[\]()#+.!|~-])', r'\\\1', subject)
        subject = html.escape(subject, quote=False).replace('@', '&#64;')
        lines.append(f'- {subject} — [{sha}](https://github.com/{REPO}/commit/{sha})')
    return '\n'.join(lines) + '\n'


def publish(tag):
    archive, feed = asset_paths(tag)
    if not archive.is_file() or not feed.is_file():
        raise ValueError('Both signed release assets must exist before publication')
    latest = github_release('latest')
    if latest and version_tuple(latest['tag_name']) >= version_tuple(tag):
        raise ValueError('Refusing to replace latest with the same or an older release')
    existing = github_release('tags/' + tag)
    if existing and not existing['draft']:
        raise ValueError('Refusing to modify an already published release')
    notes = ROOT / 'dist/release-notes.md'
    notes.write_text(release_notes(tag, latest['tag_name'] if latest else None), encoding='utf-8')
    if not existing:
        run('gh', 'release', 'create', tag, '--repo', REPO, '--verify-tag', '--draft',
            '--title', f'What the hell are you planning? {tag[1:]}', '--notes-file', notes)
    else:
        run('gh', 'release', 'edit', tag, '--repo', REPO, '--notes-file', notes)
    run('gh', 'release', 'upload', tag, archive, '--repo', REPO, '--clobber')
    run('gh', 'release', 'upload', tag, feed, '--repo', REPO, '--clobber')
    run('gh', 'release', 'edit', tag, '--repo', REPO, '--draft=false', '--latest')
    print(f'Published {tag}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--sdk', type=Path, default=ROOT / 'build/sparkle/sdk')
    parser.add_argument('--publish', action='store_true')
    args = parser.parse_args()
    version_tuple(args.tag)
    if args.publish:
        publish(args.tag)
    else:
        prepare(args.tag, args.sdk)


if __name__ == '__main__':
    main()
