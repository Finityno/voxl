from pathlib import Path
import gzip,hashlib,json,os,shutil
ROOT=Path('/Users/sondre/Documents/GitHub/voxl-fincode-streaming');P=ROOT/'media/results/2026-11-06-flow-observation';RAW=Path('/Users/sondre/Documents/Fincode Desktop Lab Evidence/2026-11-06/flow-observation-component');P.mkdir();RAW.mkdir(mode=0o700)
for label,name in [('initial','65228c11'),('final','0a252d18')]:
 folder=Path('/tmp/voxl-flow-observation-'+name);shutil.copytree(folder,RAW/label);r=json.loads((folder/'results.json').read_text());assert r['terminal'];(P/label/'logs').mkdir(parents=True)
 (P/label/'results.json').write_text(json.dumps(r,indent=2)+'\n')
 for s in r['steps']:
  assert s['groupGone'] and not s['cleanupSignals']
  try:os.killpg(s['pgid'],0)
  except ProcessLookupError:pass
  else:raise ValueError('component command group remains')
  data=(folder/s['privateLog']).read_bytes();assert hashlib.sha256(data).hexdigest()==s['logSha256'];(P/label/'logs'/(Path(s['privateLog']).name+'.gz')).write_bytes(gzip.compress(data,mtime=0))
for name in ['/tmp/voxl-flow-observation-checks-20261106.py','/tmp/voxl-flow-observation-final-checks-20261106.py',__file__]:
 p=Path(name);shutil.copy2(p,RAW/p.name);shutil.copy2(p,P/p.name)
print('Retained both exact-source component attempts')
