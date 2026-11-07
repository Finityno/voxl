"""Offline component proof, never runtime replay."""
from pathlib import Path
import gzip,hashlib,json,subprocess
p=Path(__file__).resolve().parent;groups=[]
for label in ['initial','final']:
 r=json.loads((p/label/'results.json').read_text());assert r['terminal'] and r['exitCode']==(1 if label=='initial' else 0)
 for step in r['steps']:
  assert step['groupGone'] and step['sourceCleanBefore'] and step['sourceCleanAfter'] and not step['cleanupSignals'];groups.append(step['pgid'])
  assert step['exitCode']==(101 if label=='initial' and step['name']=='strict-clippy' else 0)
  data=gzip.decompress((p/label/'logs'/(Path(step['privateLog']).name+'.gz')).read_bytes());assert hashlib.sha256(data).hexdigest()==step['logSha256']
 for name,count in [('flow-contracts',5),('producer-debt-contracts',6),('pacing-epoch-contracts',34)]:assert f'{count} passed; 0 failed' in gzip.decompress((p/label/'logs'/(name+'.log.gz')).read_bytes()).decode()
r=json.loads((p/'final/results.json').read_text());assert r['source']=='0a252d18af96ade1bf13d6319c45c61edef87238';assert len(set(groups))==18
subprocess.run(['git','diff','--exit-code',r['source'],'HEAD','--','media/src','media/Cargo.toml','media/Cargo.lock','media/vendor'],cwd=p,check=True,timeout=10)
print('Verified five flow/six debt/34 adaptive passes, strict Clippy, retained initial lint failure, hashes and 18 ended component groups. No native codec/quality/performance admission.')
