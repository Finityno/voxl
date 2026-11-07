from pathlib import Path
import gzip,hashlib,json
ROOT=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
def load(name):return json.loads((ROOT/name).read_text())
for item in load('files.json')['files']:
 raw=(ROOT/item['path']).read_bytes();assert len(raw)==item['bytes'] and sha(raw)==item['sha256'],item['path']
 assert (b'/' + b'Users' + b'/') not in (gzip.decompress(raw) if item['path'].endswith('.gz') else raw)
c=load('comparison.json');assert c['runtimeSource']=='0ec781781ba5b3063264f4f6d11dddf3f71c6bc1' and c['clippySource']=='66bd7b014e9e43d39455ef21d70d524a2af92083'
assert c['adaptiveTestsPassed']==13 and c['legacyTestsPassed']==2 and c['strictClippyPassed'] and c['coreSourceUnchangedAtClippySource'] and c['originalFileCount']==44
assert len(c['groups'])==28 and len({x['pgid'] for x in c['groups']})==28 and all(x['independentlyAbsent'] for x in c['groups'])
expected={'initial-build':('22fda7f0d935982ca4a03e5538d261cd7b91563f','test-build'),'first-runtime':('3a14eb51a9547c6160cc0f801514882acb896cc3','strict-clippy'),'smaller-burst':('9ca75abe8ce3671a0c3bf37e02e95d97517a7dce','adaptive-contracts'),'runtime-confirm':(c['runtimeSource'],'strict-clippy'),'clippy-confirm':(c['clippySource'],None)}
groups=set()
for name in c['receipts']:
 r=json.loads(gzip.decompress((ROOT/name).read_bytes()));label=Path(name).name.removesuffix('.json.gz');source,failed=expected[label]
 assert r['source']==source and r['terminal'] and r['exitCode']==(1 if failed else 0)
 assert r['scopeClarifiedAtExport'] and r['runnerOriginalLimits']
 assert sha((ROOT/('helpers/'+label+'.py')).read_bytes())==r['coordinatorExportSha256']
 for s in r['steps']:
  groups.add(s['pgid']);assert s['groupGone'] and not s['timedOut'] and s['cleanupSignals']==[] and s['sourceCleanBefore'] and s['sourceCleanAfter']
  assert s['exitCode']==s['childExitCode']==(101 if s['name']==failed else 0)
  raw=gzip.decompress((ROOT/s['privateLog']).read_bytes());assert sha(raw)==s['logSha256']
  if s['name']=='adaptive-contracts':
   counts=(10,0) if label=='first-runtime' else (11,1) if label=='smaller-burst' else (13,0)
   assert f'{counts[0]} passed; {counts[1]} failed; 0 ignored;'.encode() in raw
  if s['name'].startswith('legacy-'):
   assert b'1 passed; 0 failed; 0 ignored;' in raw and '--exact' in s['command']
 if label=='clippy-confirm':
  assert any(s['name']=='unchanged-core' and s['exitCode']==0 for s in r['steps'])
  clippy=next(s for s in r['steps'] if s['name']=='strict-clippy');assert '--examples' in clippy['command'] and '--tests' in clippy['command']
assert groups=={x['pgid'] for x in c['groups']}
p=load('platform.json');assert p['architecture']=='arm64' and p['model']=='Mac17,8' and p['osVersion']=='27.2' and '1.99.0' in p['rustc']
print('PASS: 13 adaptive + 2 legacy tests at their source, final strict Clippy/unchanged core, retained failures, 28 absent groups and all published hashes.')
