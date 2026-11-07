"""Replay exact optional-cap and retry evidence without live media traffic."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess
p=Path(__file__).resolve().parent;root=next(v for v in p.parents if (v/'media/src/webrtc.rs').is_file());parser=argparse.ArgumentParser();parser.add_argument('--archive',required=True,type=Path);raw=parser.parse_args().archive.resolve()
def read(f):return json.loads(f.read_text())
def digest(f):return hashlib.sha256(f.read_bytes()).hexdigest()
def files(base,index):
 for v in index:
  f=(base/v['path']).resolve();assert f.is_relative_to(base) and f.stat().st_size==v['bytes'] and digest(f)==v['sha256']
tracked=set(subprocess.check_output(['git','ls-files','--',str(p.relative_to(root))],cwd=root,text=True,timeout=10).splitlines());index=read(p/'files.json');files(p,index);assert all(str((p/v['path']).relative_to(root)) in tracked for v in index)
e=read(p/'evidence.json');assert e['selectedRustPassed']==30 and e['strictClippy'] and not e['newDependenciesOrFeatures'] and not e['productionAdmission'];a=read(p/'artifacts.json');assert len(a)==e['archiveOriginals']==15;files(raw,a)
groups=read(p/'cleanup.json');assert len(groups)==e['ownedGroups']==len({v['pgid'] for v in groups})==11 and all(v['independentlyAbsent'] for v in groups)
v=read(p/'receipts.json');assert v==read(raw/'current/results.json') and v['source']==e['source'] and v['scope']=='exact_quiet_cap_and_fresh_retry_component' and v['terminal'] and v['exitCode']==0
assert digest(raw/'helper.py')==v['helperSha256'] and digest(raw/'bounded-runner.py')==v['runnerSha256']
for step in v['steps']:assert step['exitCode']==step['childExitCode']==0 and step['groupGone'] and not step['cleanupSignals'] and not step['timedOut'] and step['sourceCleanBefore'] and step['sourceCleanAfter'] and digest(raw/'current'/step['privateLog'])==step['logSha256']
counts={}
for name in e['counts']:
 m=re.search(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;', (raw/'current/private'/(name+'.log')).read_text());assert m;counts[name]=int(m[1])
assert counts==e['counts'] and sum(counts.values())==30 and any(step['name']=='media-tests-strict-clippy' for step in v['steps'])
log=(raw/'current/private/probe-network.log').read_text()
def fields(line):return {k:(int(v) if v.isdigit() else v) for k,v in re.findall(r'([a-z_]+)=([A-Za-z_]+|[0-9]+)',line)}
success=[fields(row) for row in log.splitlines() if row.lstrip('.').startswith('quiet_retry_success ')];failed=[fields(row) for row in log.splitlines() if row.lstrip('.').startswith('quiet_retry_failure ')];assert len(success)==1 and len(failed)==3
r=success[0];assert r['attempts']==2 and r['fresh_arrivals']==32 and r['padding_received']>=63 and r['padding_original_rtp_bytes']==17408 and r['probe_us']<500000 and r['resume_bytes']==64005 and r['total_us']<750000 and r['queue_max_us']<750000 and r['dropped']==1 and r['overflow']==0
assert {r['fault'] for r in failed}=={'Close','Twice','Slow'}
for r in failed:
 assert r['elapsed_us']<750000 and r['fake_video_frames']==r['overflow']==0 and r['dropped']==(2 if r['fault']=='Twice' else 1)
 if r['fault']=='Slow':assert r['elapsed_us']>=490000
subprocess.run(['git','diff','--exit-code',e['source'],'--','media/src','media/vendor/rtc/src','media/Cargo.toml','media/Cargo.lock','Cargo.toml','Cargo.lock'],cwd=root,check=True,timeout=15)
print('Verified exact optional quiet-cap observation, 30 selected tests/strict Clippy, fresh retry/loss/close/original deadline; 15 originals and 11 independently absent groups. No native quality/resources, billing or default admission.')
