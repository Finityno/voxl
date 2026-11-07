"""Offline exact-source native component receipts; no encoder execution."""
from pathlib import Path
import gzip,hashlib,json,subprocess
p=Path(__file__).resolve().parent;groups=[]
for label,source in [('initial','842dfeb7dcf921f77e024ca791b32a44bb0abe52'),('sparse','56a4b318c576b0ac7c6da1ac5616a5fcbe90d896')]:
 r=json.loads((p/label/'results.json').read_text());assert r['source']==source and r['terminal'] and r['exitCode']==0
 for s in r['steps']:
  assert s['exitCode']==0 and s['groupGone'] and s['sourceCleanBefore'] and s['sourceCleanAfter'] and not s['cleanupSignals'];groups.append(s['pgid'])
  data=gzip.decompress((p/label/'logs'/(Path(s['privateLog']).name+'.gz')).read_bytes());assert hashlib.sha256(data).hexdigest()==s['logSha256']
 assert '4 passed; 0 failed' in gzip.decompress((p/label/'logs/keyframe-policy-contracts.log.gz').read_bytes()).decode()
assert len(set(groups))==14
subprocess.run(['git','diff','--exit-code','56a4b318c576b0ac7c6da1ac5616a5fcbe90d896','HEAD','--','media/src','media/Cargo.toml','media/Cargo.lock','media/vendor'],cwd=p,check=True,timeout=10)
print('Verified four native keyframe-policy tests on three real providers, including forced/static/sparse-time output, strict Clippy, hashes and14 ended groups. No transport/quality/resource/default admission.')
