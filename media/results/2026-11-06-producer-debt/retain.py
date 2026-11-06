from pathlib import Path
import gzip,hashlib,json,os,shutil
root=Path('/Users/sondre/Documents/GitHub/voxl-fincode-streaming');out=root/'media/results/2026-11-06-producer-debt';raw=Path('/Users/sondre/Documents/Fincode Desktop Lab Evidence/2026-11-06/producer-debt-component');source=Path('/tmp/voxl-producer-debt-fe094ddc')
out.mkdir();raw.mkdir(mode=0o700);shutil.copytree(source,raw/'run');shutil.copy2('/tmp/voxl-producer-debt-checks-20261106.py',raw/'checks.py');shutil.copy2(__file__,raw/'retain.py')
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
r=json.loads((source/'results.json').read_text());assert r['terminal'] and r['exitCode']==0 and not (source/'active-group.json').exists();(out/'logs').mkdir()
for step in r['steps']:
 assert step['exitCode']==0 and step['groupGone'] and not step['cleanupSignals'] and step['sourceCleanBefore'] and step['sourceCleanAfter']
 try:os.killpg(step['pgid'],0)
 except ProcessLookupError:pass
 else:raise ValueError('owned command remains or pid reused')
 data=(source/step['privateLog']).read_bytes();assert hashlib.sha256(data).hexdigest()==step['logSha256'];(out/'logs'/(Path(step['privateLog']).name+'.gz')).write_bytes(gzip.compress(data,mtime=0))
write(out/'results.json',r);write(out/'cleanup.json',{'groups':len(r['steps']),'independentlyAbsent':True,'pids':[step['pgid'] for step in r['steps']]})
shutil.copy2('/tmp/voxl-producer-debt-checks-20261106.py',out/'checks.py');shutil.copy2(__file__,out/'retain.py')
print('Retained component commands:',len(r['steps']))
