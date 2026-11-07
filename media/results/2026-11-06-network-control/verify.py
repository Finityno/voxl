from pathlib import Path
import gzip,hashlib,json,re
p=Path(__file__).resolve().parent
for row in json.loads((p/'files.json').read_text()):
 b=(p/row['path']).read_bytes();assert len(b)==row['bytes'] and hashlib.sha256(b).hexdigest()==row['sha256'],row['path']
 s=gzip.decompress(b).decode() if row['path'].endswith('.gz') else b.decode()
 assert not re.search('/'+'Users/'+r'[^/]+/',s),row['path']
 assert not re.search(r'(?<![A-Za-z0-9])#[0-9]|[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+#',s),row['path']
c=json.loads((p/'comparison.json').read_text());assert c['source']=='3495070e63ed455ba3cc2cc5953392f9b7a822fb' and c['testOnly'] and not c['latencyAdmission'] and not c['defaultChanged']
assert len(c['groups'])==27 and all(r['independentlyAbsent'] for r in c['groups']) and c['originalFileCount']>=36
for run in c['executions']:
 d=json.loads(gzip.decompress((p/run['receipt']).read_bytes()));assert d['source']==run['source'] and d['terminal']
 assert all(s['sourceCleanBefore'] and s['sourceCleanAfter'] and s['groupGone'] and not s['timedOut'] and not s['cleanupSignals'] for s in d['steps'])
 steps={s['name']:s for s in d['steps']};assert steps['test-build']['exitCode']==0
 log=gzip.decompress((p/steps['adaptive-contracts']['exportedLog']).read_bytes()).decode();assert f"{run['passed']} passed; {run['failed']} failed" in log
 assert steps['adaptive-contracts']['exitCode']==(101 if run['failed'] else 0)
 if run['job']=='final':assert d['exitCode']==0 and steps['strict-clippy']['exitCode']==0
 elif run['job']=='lint-feedback':assert d['exitCode']==1 and steps['strict-clippy']['exitCode']==101
 else:assert d['exitCode']==1 and 'strict-clippy' not in steps
b,=c['bottleneck'];assert b['send_us']<500000<b['receiver_us'] and b['peak_queue']<=256 and b['drops']==b['duplicates']==0
loss,=c['loss'];assert loss['dropped']==1 and loss['duplicates']>=1 and loss['peak_queue']<=256
stages=c['capacity'];assert [s['stage'] for s in stages]==[0,1,2] and [s['capacity_bps'] for s in stages]==[3000000,500000,3000000]
assert all(s['overflow']==0 and s['peak_queue']<=256 for s in stages)
assert stages[1]['media_min_bps']<1000000 and stages[2]['recovery_final_bps']>stages[1]['media_min_bps']*1.1
assert stages[1]['queue_max_us']>3000000 and stages[2]['sent_frames']==307
assert 0<len(c['targetTrace'])<=32 and all(250<=s['media_kbps']<=1500 for s in c['targetTrace'])
print('Verifiedfour real encrypted-UDP/fixture checks, strict Clippy, retained failures,27 absent groups and explicit slow-queue counterevidence; no latency admission or native replay.')
