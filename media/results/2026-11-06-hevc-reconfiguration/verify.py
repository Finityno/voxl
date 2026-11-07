from pathlib import Path
import gzip,hashlib,json,subprocess
p=Path(__file__).resolve().parent
root=next(parent for parent in p.parents if (parent/'.git').exists())
tracked=set(subprocess.check_output(['git','ls-files','--',str(p.relative_to(root))],cwd=root,text=True,timeout=10).splitlines())
for row in json.loads((p/'files.json').read_text()):
 file=p/row['path'];assert str(file.relative_to(root)) in tracked and file.stat().st_size==row['bytes'] and hashlib.sha256(file.read_bytes()).hexdigest()==row['sha256']
r=json.loads((p/'receipt.json').read_text());assert r['source']=='cd1bd368d67a40d208f78e30dd25f05a41aead0e' and r['terminal'] and r['exitCode']==0
for step in r['steps']:
 raw=gzip.decompress((p/'logs'/(Path(step['privateLog']).name+'.gz')).read_bytes())
 assert hashlib.sha256(raw).hexdigest()==step['logSha256'] and step['exitCode']==0 and step['groupGone'] and not step['cleanupSignals'] and step['sourceCleanBefore'] and step['sourceCleanAfter']
 assert not step['timedOut']
 if step['name']=='hevc-profile-contracts':assert '2 passed; 0 failed' in raw.decode()
c=json.loads((p/'cleanup.json').read_text());assert c['independentlyAbsent'] and len(c['groups'])==6 and c['groups']==[row['pgid'] for row in r['steps']]
print('Verified two HEVC profile checks, strict Clippy and six absent groups; metadata evidence only.')
