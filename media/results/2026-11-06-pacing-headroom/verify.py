from pathlib import Path
import gzip,hashlib,json,re
p=Path(__file__).resolve().parent
for row in json.loads((p/'files.json').read_text()):
 b=(p/row['path']).read_bytes();assert len(b)==row['bytes'] and hashlib.sha256(b).hexdigest()==row['sha256'],row['path']
 s=gzip.decompress(b).decode() if row['path'].endswith('.gz') else b.decode()
 assert not re.search('/'+'Users/'+r'[^/]+/',s),row['path']
 assert not re.search(r'(?<![A-Za-z0-9])#[0-9]|[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+#',s),row['path']
c=json.loads((p/'comparison.json').read_text());assert len(c['groups'])==13 and all(r['independentlyAbsent'] for r in c['groups'])
for name,success in [('initial-clock-failure',False),('responsive-pass',True)]:
 d=json.loads(gzip.decompress((p/'receipts'/(name+'.json.gz')).read_bytes()));assert d['terminal'] and d['source']==c['source' if success else 'initialSource']
 assert d['exitCode']==(0 if success else 1)
 assert all(s['sourceCleanBefore'] and s['sourceCleanAfter'] and s['groupGone'] and not s['cleanupSignals'] and not s['timedOut'] for s in d['steps'])
 steps={s['name']:s for s in d['steps']}
 if success:
  assert len(d['steps'])==8 and all(s['exitCode']==0 for s in d['steps'])
  for n,count in [('adaptive-contracts',21),('legacy-loopback',1),('legacy-epoch',1)]:
   log=gzip.decompress((p/steps[n]['exportedLog']).read_bytes()).decode();assert re.search(fr'test result: ok\. {count} passed; 0 failed; 0 ignored',log)
 else:
  assert steps['test-build']['exitCode']==0 and steps['adaptive-contracts']['exitCode']==101
  log=gzip.decompress((p/steps['adaptive-contracts']['exportedLog']).read_bytes()).decode();assert '20 passed; 1 failed' in log
print('Verified21 adaptive/two legacy passes, strict Clippy, retained virtual-clock failure and13 independently absent groups; no native replay.')
