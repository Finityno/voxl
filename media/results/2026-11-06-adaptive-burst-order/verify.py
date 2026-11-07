from pathlib import Path
import gzip,hashlib,json,re
p=Path(__file__).resolve().parent
for row in json.loads((p/'files.json').read_text()):
 raw=(p/row['path']).read_bytes();assert len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256']
 text=gzip.decompress(raw).decode() if row['path'].endswith('.gz') else raw.decode()
 assert not re.search('/'+'Users/'+r'[^/]+/',text)
 assert not re.search(r'(?<![A-Za-z0-9])#[0-9]|[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+#',text)
d=json.loads(gzip.decompress((p/'receipt.json.gz').read_bytes()));assert d['terminal'] and d['exitCode']==0 and len(d['steps'])==6
assert d['source']=='9e65865c425d6e7cbe4044381de3d2a1f3d3d26c' and all(s['exitCode']==0 and s['groupGone'] and s['independentlyAbsent'] and not s['timedOut'] for s in d['steps'])
assert 'test result: ok. 3 passed; 0 failed; 0 ignored' in gzip.decompress((p/'adaptive-contracts.log.gz').read_bytes()).decode()
print('Verified three native component passes, strict Clippy and six absent-group receipts; no runtime replay.')
