#!/usr/bin/env python3
"""Verify bundled app snapshots, or refresh them from an official signed catalogue.

Development belongs in each app's repository. Bundles retain pinned release
snapshots so first boot and offline installations do not require network access.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT/'store-apps.lock.json'
FOLDERS = {'dev.cartridge.frequency':'frequency', 'dev.cartridge.mission-control':'mission_control', 'dev.cartridge.outside':'outside'}

def safe_name(name):
    path = PurePosixPath(name)
    if not name or path.is_absolute() or '..' in path.parts or '\\' in name or str(path) != name:
        raise ValueError(f'Unsafe package path: {name!r}')
    return name

def files_from_archive(data):
    files = {}; total = 0
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
        for member in tar:
            name = safe_name(member.name)
            if member.isdir(): continue
            if not member.isfile() or name in files:
                raise ValueError('Links, special files or duplicates in package')
            total += member.size
            if total > 128*1024*1024 or len(files) >= 4096:
                raise ValueError('Expanded package exceeds limits')
            files[name] = tar.extractfile(member).read()
    if 'main.lua' not in files or 'cartridge.json' not in files:
        raise ValueError('Package is not a cartridge')
    return files

def verify_catalog(path):
    envelope = json.loads(path.read_text())
    if envelope['key_id'] != 'cartridge-official-v1': raise ValueError('Unknown catalogue key')
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        (tmp/'payload').write_bytes(envelope['payload'].encode())
        (tmp/'signature').write_bytes(bytes.fromhex(envelope['signature']))
        subprocess.run([os.environ.get('OPENSSL','openssl'),'pkeyutl','-verify','-rawin','-pubin','-inkey',str(ROOT/'scripts/store-public-key.pem'),'-in',str(tmp/'payload'),'-sigfile',str(tmp/'signature')],check=True,capture_output=True)
    catalog = json.loads(envelope['payload'])
    if catalog['version'] != 2: raise ValueError('Unsupported catalogue version')
    return catalog

def check():
    lock = json.loads(LOCK.read_text())
    for app in lock['apps']:
        root = ROOT/'lua_cartridges'/FOLDERS[app['id']]
        for name, expected in app['files'].items():
            path = root/safe_name(name)
            if path.is_symlink() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
                raise ValueError(f'Bundled source differs from released package: {path.relative_to(ROOT)}. Change it in the app repo and refresh the snapshot.')
    print(f'Verified {len(lock["apps"])} bundled app snapshots against release pins')

def refresh(path):
    catalog = verify_catalog(path)
    previous = json.loads(LOCK.read_text()) if LOCK.exists() else {'apps':[]}
    old = {app['id']:app for app in previous['apps']}
    prepared = []
    for app in catalog['apps']:
        if app['id'] not in FOLDERS: continue
        package = app['package']
        if not package['url'].startswith('https://') or not 0 < package['size'] <= 32*1024*1024:
            raise ValueError('Invalid package transport or size')
        with urllib.request.urlopen(urllib.request.Request(package['url'],headers={'User-Agent':'CartridgeOS-BundleSync/1'}),timeout=60) as response:
            if response.status != 200 or not response.url.startswith('https://'): raise ValueError('Invalid download response')
            data = response.read(32*1024*1024+1)
        if len(data) != package['size'] or hashlib.sha256(data).hexdigest() != package['sha256']:
            raise ValueError('Package checksum/size mismatch')
        files = files_from_archive(data)
        manifest = json.loads(files['cartridge.json'])
        if any(manifest[k] != app[k] for k in ['id','version','permissions']): raise ValueError('Manifest identity mismatch')
        prepared.append((app,files))
    if {a['id'] for a,_ in prepared} != set(FOLDERS): raise ValueError('Catalogue is missing one of the three bundled apps')
    entries = []
    registry = json.loads((ROOT/'registry.json').read_text())
    for app,files in prepared:
        root = ROOT/'lua_cartridges'/FOLDERS[app['id']]
        for obsolete in set(old.get(app['id'],{}).get('files',{}))-set(files):
            (root/safe_name(obsolete)).unlink(missing_ok=True)
        for name,content in files.items():
            target = root/name
            if target.is_symlink(): raise ValueError('Refusing to replace a symlink')
            target.parent.mkdir(parents=True,exist_ok=True)
            target.write_bytes(content)
        entries.append({'id':app['id'],'repo_url':app['repo_url'],'version':app['version'],'package':app['package'],
                        'files':{name:hashlib.sha256(body).hexdigest() for name,body in sorted(files.items())}})
        registry['apps'] = [app if item['id']==app['id'] else item for item in registry['apps']]
    LOCK.write_text(json.dumps({'version':1,'apps':entries},indent=2)+'\n')
    (ROOT/'registry.json').write_text(json.dumps(registry,indent=2)+'\n')
    check()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--refresh',type=Path,help='Explicitly import packages from this signed catalogue file')
    args=parser.parse_args()
    if args.refresh: refresh(args.refresh)
    else: check()
if __name__=='__main__':main()
