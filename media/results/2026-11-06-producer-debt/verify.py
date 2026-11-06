"""Offline receipt/hash verification only; no native execution."""
from pathlib import Path
import gzip,hashlib,json,subprocess
p=Path(__file__).resolve().parent
r=json.loads((p/'results.json').read_text());assert r['source']=='fe094ddc75c635a67fdb24c2a95880f6c216b180' and r['terminal'] and r['exitCode']==0
for step in r['steps']:
 assert step['exitCode']==0 and step['groupGone'] and not step['cleanupSignals'] and step['sourceCleanBefore'] and step['sourceCleanAfter']
 raw=gzip.decompress((p/'logs'/(Path(step['privateLog']).name+'.gz')).read_bytes());assert hashlib.sha256(raw).hexdigest()==step['logSha256']
for name,count in [('producer-debt-contracts',6),('pacing-epoch-contracts',34)]:assert f'{count} passed; 0 failed' in gzip.decompress((p/'logs'/(name+'.log.gz')).read_bytes()).decode()
c=json.loads((p/'cleanup.json').read_text());assert c['groups']==len(r['steps'])==len(set(c['pids']))==8 and c['independentlyAbsent']
subprocess.run(['git','diff','--exit-code',r['source'],'HEAD','--','media/src','media/Cargo.toml','media/Cargo.lock','media/vendor'],cwd=p,check=True,timeout=10)
print('Verified exact-source six debt/34 adaptive passes, strict Clippy, log hashes and eight absent command groups. No native encoder/decoder or network-performance admission.')
