"""Verify the full fresh-cluster retry archive without generating network traffic."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess
proof=Path(__file__).resolve().parent;root=next(v for v in proof.parents if (v/'media/src/webrtc.rs').is_file())
parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);raw=parser.parse_args().archive.resolve()
def read(p):return json.loads(p.read_text())
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def check(base,items):
 for item in items:
  p=(base/item['path']).resolve();assert p.is_relative_to(base) and p.stat().st_size==item['bytes'] and digest(p)==item['sha256']
tracked=set(subprocess.check_output(['git','ls-files','--',str(proof.relative_to(root))],cwd=root,text=True,timeout=10).splitlines());index=read(proof/'files.json');check(proof,index);assert all(str((proof/v['path']).relative_to(root)) in tracked for v in index)
e=read(proof/'evidence.json');assert e['selectedRustPassed']==29 and e['strictClippy'] and not e['productionDefaultEnabled'] and not e['newDependenciesOrFeatures']
a=read(proof/'artifacts.json');assert len(a)==e['archiveOriginals']==41;check(raw,a)
groups=read(proof/'cleanup.json');assert len(groups)==e['ownedGroups']==len({v['pgid'] for v in groups})==30 and all(v['independentlyAbsent'] for v in groups)
for item in read(proof/'receipts.json'):
 label,value=item['job'],item['receipt'];assert value==read(raw/label/'results.json') and value['terminal'] and value['exitCode']==(1 if label.startswith('failed-') else 0)
 assert digest(raw/(label+'-helper.py'))==value['helperSha256'] and digest(raw/'bounded-runner.py')==value['runnerSha256']
 for i,step in enumerate(value['steps']):
  assert step['exitCode']==step['childExitCode']==(101 if label.startswith('failed-') and i==len(value['steps'])-1 else 0)
  assert step['groupGone'] and not step['timedOut'] and not step['cleanupSignals'] and step['sourceCleanBefore'] and step['sourceCleanAfter'] and digest(raw/label/step['privateLog'])==step['logSha256']
current=read(raw/'current/results.json');assert current['source']==e['source'] and current['scope']=='fresh_complete_cluster_quiet_retry_component'
counts={}
for name in e['counts']:
 m=re.search(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;', (raw/'current/private'/(name+'.log')).read_text());assert m;counts[name]=int(m[1])
assert counts==e['counts'] and sum(counts.values())==29
assert any(v['name']=='media-tests-strict-clippy' and v['exitCode']==0 for v in current['steps'])
failed=(raw/'failed-byte-hook/private/probe-network.log').read_text();assert '9 passed; 1 failed; 0 ignored;' in failed and 'left: 16896' in failed and 'right: 17408' in failed
sdk=(raw/'sdk-cc-interceptor.rs').read_text();assert 'self.history.add_outgoing(' in sdk and 'rtp_packet.marshal_size(),' in sdk
preflight=read(raw/'publish-preflight.json');assert preflight['validationStarted'] is False
log=(raw/'current/private/probe-network.log').read_text()
def fields(line):return {k:(int(v) if v.isdigit() else v) for k,v in re.findall(r'([a-z_]+)=([A-Za-z_]+|[0-9]+)',line)}
assert [fields(v) for v in log.splitlines() if v.lstrip('.').startswith('quiet_retry_success ')]==e['success']
assert [fields(v) for v in log.splitlines() if v.lstrip('.').startswith('quiet_retry_failure ')]==e['failures']
assert len(e['success'])==1 and len(e['failures'])==3
v=e['success'][0];assert v['attempts']==2 and v['fresh_arrivals']==32 and v['padding_received']>=63 and v['padding_original_rtp_bytes']==17408 and v['probe_us']<500000 and v['total_us']<750000 and v['resume_bytes']==64005 and v['queue_max_us']<750000 and v['dropped']==1 and v['overflow']==0
assert {v['fault'] for v in e['failures']}=={'Close','Twice','Slow'}
for v in e['failures']:
 assert v['elapsed_us']<750000 and v['fake_video_frames']==v['overflow']==0 and v['dropped']==(2 if v['fault']=='Twice' else 1)
 if v['fault']=='Slow':assert v['elapsed_us']>=490000
source=subprocess.check_output(['git','show',e['source']+':media/src/webrtc.rs'],cwd=root,text=True,timeout=10);change=e['sourceDocOnlyReplacement'];assert source.count(change['old'])==1 and (root/'media/src/webrtc.rs').read_text()==source.replace(change['old'],change['new'])
subprocess.run(['git','diff','--exit-code',e['source'],'--','media/src','media/vendor/rtc/src','media/Cargo.toml','media/Cargo.lock','Cargo.toml','Cargo.lock',':(exclude)media/src/webrtc.rs'],cwd=root,check=True,timeout=15)
subprocess.run(['git','diff','--exit-code','2a5458b389b301586d5412b46c83f26d36d503a3',e['source'],'--','media/Cargo.toml','media/Cargo.lock','Cargo.toml','Cargo.lock'],cwd=root,check=True,timeout=15)
print('Verified 29 selected Rust tests/strict Clippy, full fresh-cluster retry, second loss/close/original-deadline failure, exact post-TWCC original bytes; 41 originals and 30 independently absent groups. One exact Rustdoc-only replacement; no native quality/resources, physical endpoint, billing or default admission.')
