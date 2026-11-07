from pathlib import Path
import gzip,hashlib,json,os,shutil
ROOT=Path('/Users/sondre/Documents/GitHub/voxl-fincode-streaming');P=ROOT/'media/results/2026-11-06-keyframe-policy';RAW=Path('/Users/sondre/Documents/Fincode Desktop Lab Evidence/2026-11-06/keyframe-policy-component');P.mkdir();RAW.mkdir(mode=0o700)
for label,name in [('initial','842dfeb7'),('sparse','56a4b318')]:
 folder=Path('/tmp/voxl-keyframe-policy-'+name);shutil.copytree(folder,RAW/label);r=json.loads((folder/'results.json').read_text());assert r['terminal'] and r['exitCode']==0;(P/label/'logs').mkdir(parents=True)
 (P/label/'results.json').write_text(json.dumps(r,indent=2)+'\n')
 for s in r['steps']:
  assert s['exitCode']==0 and s['groupGone'] and not s['cleanupSignals']
  try:os.killpg(s['pgid'],0)
  except ProcessLookupError:pass
  else:raise ValueError('component group remains')
  data=(folder/s['privateLog']).read_bytes();assert hashlib.sha256(data).hexdigest()==s['logSha256'];(P/label/'logs'/(Path(s['privateLog']).name+'.gz')).write_bytes(gzip.compress(data,mtime=0))
for name in ['/tmp/voxl-keyframe-policy-checks-20261106.py','/tmp/voxl-keyframe-duration-checks-20261106.py',__file__]:
 f=Path(name);shutil.copy2(f,RAW/f.name);shutil.copy2(f,P/f.name)
print('Retained two component executions')
