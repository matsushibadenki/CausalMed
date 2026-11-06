#!/usr/bin/env python3
"""Prepare a relocatable worker runtime; no development packages are copied."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / 'src-tauri/python-worker'
CACHE = ROOT / '.worker-cache'
TARGET = 'aarch64-apple-darwin'


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def fetch(url, sha, path, offline):
    if path.is_file() and digest(path) == sha:
        return path
    if offline:
        raise RuntimeError('Missing or changed cached artifact: ' + path.name)
    if not url.startswith(('https://github.com/astral-sh/python-build-standalone/', 'https://files.pythonhosted.org/')):
        raise RuntimeError('Unexpected artifact origin')
    path.parent.mkdir(parents=True, exist_ok=True)
    pending = path.with_suffix(path.suffix + '.pending')
    try:
        with urllib.request.urlopen(url, timeout=60) as response, pending.open('wb') as output:
            shutil.copyfileobj(response, output)
        if digest(pending) != sha:
            raise RuntimeError('Artifact SHA-256 mismatch: ' + path.name)
        pending.replace(path)
    finally:
        pending.unlink(missing_ok=True)
    return path


def inventory(folder):
    rows = {}
    for p in sorted(folder.rglob('*')):
        if p.is_symlink() and not p.resolve().is_relative_to(folder.resolve()):
            raise RuntimeError('External runtime symlink: ' + str(p))
        if p.is_file() and p.relative_to(folder).as_posix() not in ('runtime.json', 'files.json'):
            rows[p.relative_to(folder).as_posix()] = digest(p)
    return rows


def verify(folder, pin, lock_sha):
    try:
        m = json.loads((folder/'runtime.json').read_text())
        assert m['schemaVersion'] == 1 and m['target'] == TARGET
        assert m['pythonVersion'] == pin['pythonVersion'] and m['archiveSha256'] == pin['sha256']
        assert m['dependencyLockSha256'] == lock_sha
        assert m['buildRecipeSha256'] == digest(Path(__file__))
        assert digest(folder/'python/bin/python3.12') == m['pythonSha256']
        assert digest(folder/'files.json') == m['inventorySha256']
        assert inventory(folder) == json.loads((folder/'files.json').read_text())
        return m
    except (OSError, KeyError, ValueError, AssertionError):
        return None


def run(python, *args):
    env = {k: v for k, v in os.environ.items() if k not in ('PYTHONPATH', 'PYTHONHOME', 'VIRTUAL_ENV', 'PYTHONUSERBASE')}
    subprocess.run([str(python), '-I', '-B', *args], env=env, check=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--offline', action='store_true')
    parser.add_argument('--verify', action='store_true')
    parser.add_argument('--rebuild', action='store_true')
    args = parser.parse_args()
    if platform.system() != 'Darwin' or platform.machine() not in ('arm64', 'aarch64'):
        raise RuntimeError('Bundled runtime currently supports Apple-silicon macOS only')
    target = os.environ.get('TAURI_ENV_TARGET_TRIPLE', TARGET)
    if target != TARGET:
        raise RuntimeError('No pinned worker runtime for build target: ' + target)
    pin = json.loads((ROOT/'python/runtime'/f'{TARGET}.json').read_text())
    lock_path = ROOT/'python/runtime'/f'wheels-{TARGET}.json'
    lock = json.loads(lock_path.read_text())
    lock_sha = digest(lock_path)
    normalize = lambda n: n.lower().replace('_', '-').replace('.', '-')
    requirements = {normalize(n): v for n, v in (l.split('==') for l in (ROOT/'python/requirements.lock.txt').read_text().splitlines() if l.strip())}
    assert requirements == {normalize(p['name']): p['version'] for p in lock['packages']}, 'Dependency locks disagree'
    if not args.rebuild and verify(DEST, pin, lock_sha):
        print('Verified bundled Python', pin['pythonVersion'], TARGET)
        return
    if args.verify:
        raise RuntimeError('Bundled runtime missing or modified; run npm run bundle:worker')
    archive = fetch(pin['url'], pin['sha256'], CACHE/'downloads'/f'python-{TARGET}.tar.gz', args.offline)
    wheels = [fetch(p['url'], p['sha256'], CACHE/'wheels'/p['file'], args.offline) for p in lock['packages']]
    CACHE.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='runtime-', dir=CACHE) as pending:
        stage = Path(pending)/'python-worker'
        stage.mkdir()
        with tarfile.open(archive) as tar:
            if not hasattr(tarfile, 'data_filter'):
                raise RuntimeError('Build host requires Python with tarfile.data_filter (3.10.12+)')
            tar.extractall(stage, filter='data')
        python = stage/'python/bin/python3.12'
        hashed = CACHE/'pinned-dependencies.txt'
        hashed.write_text(''.join(p['name']+'=='+p['version']+' --hash=sha256:'+p['sha256']+'\n' for p in lock['packages']))
        run(python, '-m', 'pip', '--isolated', '--no-cache-dir', 'install', '--no-index', '--find-links', str(CACHE/'wheels'), '--no-deps', '--require-hashes', '--only-binary=:all:', '--no-compile', '--disable-pip-version-check', '-r', str(hashed))
        run(python, '-m', 'pip', '--isolated', '--no-cache-dir', 'check')
        run(python, '-c', "import sys,anndata,h5py,numpy,pandas,scipy,importlib.metadata; assert sys.version.split()[0]=='"+pin['pythonVersion']+"'; assert importlib.metadata.version('anndata')=='"+requirements['anndata']+"'")
        # Only the explicitly launched interpreter is shipped; entrypoint shebangs
        # from wheel installation would otherwise refer to the temporary build path.
        for entry in (stage/'python/bin').iterdir():
            if entry.name != 'python3.12':
                entry.unlink()
        for entry in stage.rglob('*.pyc'):
            entry.unlink()
        for entry in sorted(stage.rglob('__pycache__'), reverse=True):
            shutil.rmtree(entry)
        notices = '# Bundled worker notices\n\nPython Standalone: '+pin['url']+'\n\nCPython license: python/lib/python3.12/LICENSE.txt. Package license files and native-library notices remain in their wheel distributions under python/lib/python3.12/site-packages.\n\n'
        notices += '\n'.join('- '+p['name']+' '+p['version']+' — '+p['url'] for p in lock['packages'])+'\n'
        (stage/'THIRD_PARTY_NOTICES.md').write_text(notices)
        (stage/'files.json').write_text(json.dumps(inventory(stage), sort_keys=True, indent=1)+'\n')
        m = dict(schemaVersion=1, target=TARGET, minimumMacOS='14.0', pythonVersion=pin['pythonVersion'], anndataVersion=requirements['anndata'], archiveSha256=pin['sha256'], dependencyLockSha256=lock_sha, pythonSha256=digest(python), inventorySha256=digest(stage/'files.json'), buildRecipeSha256=digest(Path(__file__)))
        (stage/'runtime.json').write_text(json.dumps(m, indent=2)+'\n')
        assert verify(stage, pin, lock_sha), 'Prepared inventory verification failed'
        # Publish only a fully verified generated directory; preserve the previous one on failure.
        old = CACHE/'previous-runtime'
        if old.exists():
            shutil.rmtree(old)
        if DEST.exists():
            DEST.replace(old)
        try:
            stage.replace(DEST)
        except OSError:
            if old.exists():
                old.replace(DEST)
            raise
        if old.exists():
            shutil.rmtree(old)
    print('Prepared bundled Python', pin['pythonVersion'], TARGET)


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, AssertionError, subprocess.CalledProcessError) as e:
        raise SystemExit(str(e))
