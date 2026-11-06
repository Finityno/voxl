from pathlib import Path
import gzip,hashlib,json,re
root=Path(__file__).resolve().parent
index=json.loads((root/'files.json').read_text())
for row in index:
 raw=(root/row['path']).read_bytes()
 assert len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
 text=gzip.decompress(raw).decode() if row['path'].endswith('.gz') else raw.decode()
 assert not re.search(r'(?<![A-Za-z0-9])#[0-9]|[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+#',text),row['path']
 assert not re.search('/'+'Users/'+r'[^/]+/',text),row['path']
c=json.loads((root/'comparison.json').read_text());assert len(c['groups'])==12 and all(x['independentlyAbsent'] for x in c['groups'])
success=json.loads(gzip.decompress((root/'receipts/worker-confirm.json.gz').read_bytes()));assert success['terminal'] and success['exitCode']==0 and success['source']==c['source']
assert len(success['steps'])==8 and all(x['exitCode']==0 and x['groupGone'] and not x['timedOut'] for x in success['steps'])
for name,count in [('adaptive-contracts',14),('legacy-h264',1),('legacy-hevc',1)]:
 row=next(s for s in success['steps'] if s['name']==name)
 text=gzip.decompress((root/row['privateLog']).read_bytes()).decode()
 assert re.search(fr'test result: ok\. {count} passed; 0 failed; 0 ignored',text)
failure=json.loads(gzip.decompress((root/'receipts/lock-failure.json.gz').read_bytes()));assert failure['terminal'] and failure['exitCode']==1
assert next(s for s in failure['steps'] if s['name']=='test-build')['exitCode']==75
print('Verified fourteen adaptive and two legacy passes, strict Clippy, retained lock failure and twelve absent-group receipts; no runtime replay.')
