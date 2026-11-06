from pathlib import Path
import gzip,hashlib,json,re
p=Path(__file__).resolve().parent
for row in json.loads((p/'files.json').read_text()):
 b=(p/row['path']).read_bytes();assert len(b)==row['bytes'] and hashlib.sha256(b).hexdigest()==row['sha256'],row['path']
 s=gzip.decompress(b).decode() if row['path'].endswith('.gz') else b.decode();assert not re.search('/'+'Users/'+r'[^/]+/',s),row['path']
 assert not re.search(r'(?<![A-Za-z0-9])#[0-9]|[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+#',s),row['path']
c=json.loads((p/'comparison.json').read_text());assert c['source']=='b21fb65eb97fe34ede1ce46f218dfb34a5824d6d' and not c['defaultControllerChanged'] and not c['independentNativePhysicalPerformanceAdmission']
assert len(c['groups'])==39 and all(s['independentlyAbsent'] for s in c['groups']) and c['originalFileCount']==58
for run in c['executions']:
 d=json.loads(gzip.decompress((p/run['receipt']).read_bytes()));assert d['terminal'] and d['source']==run['source'] and d['exitCode']==run['exitCode']
 assert all(s['sourceCleanBefore'] and s['sourceCleanAfter'] and s['groupGone'] and not s['timedOut'] and not s['cleanupSignals'] for s in d['steps'])
 steps={s['name']:s for s in d['steps']}
 if run['job']=='final-contracts':
  assert d['exitCode']==0 and steps['strict-clippy']['exitCode']==0
  assert '34 passed; 0 failed' in gzip.decompress((p/steps['adaptive-contracts']['exportedLog']).read_bytes()).decode()
 elif run['job']=='independent-repeats':
  assert d['exitCode']==0
  for name in ['transient-repeat-one','transient-repeat-two']:assert steps[name]['exitCode']==0 and '1 passed; 0 failed' in gzip.decompress((p/steps[name]['exportedLog']).read_bytes()).decode()
 elif run['job']=='initial-build':assert steps['test-build']['exitCode']==101 and 'fast-transient' not in steps
 elif run['job']=='trend20-lint':assert steps['fast-transient']['exitCode']==0 and steps['strict-clippy']['exitCode']==101
 else:
  assert d['exitCode']==1
  name='adaptive-contracts' if run['job']=='baseline-trace' else 'fast-transient';assert steps[name]['exitCode']==101
assert len(c['fastRuns'])==3
for run in c['fastRuns']:assert run['first_reduction_us']<=c['newGate']['firstReductionMicrosMax'] and run['max_residence_us']<=c['newGate']['queueResidenceMicrosMax']
assert c['sameSourceBaseline']['first_reduction_us']>1500000 and c['sameSourceBaseline']['max_residence_us']>3000000
print('Verified34 adaptive passes, strict Clippy, three final transient observations, retained failed candidates,39 absent groups and explicit scope; no native replay/default admission.')
