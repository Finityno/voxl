"""Replay sparse-capacity evidence without capture, transport or Cargo."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess
p=Path(__file__).resolve().parent;root=next(v for v in p.parents if (v/'media/src/webrtc.rs').is_file());parser=argparse.ArgumentParser();parser.add_argument('--archive',required=True,type=Path);raw=parser.parse_args().archive.resolve()
def read(f):return json.loads(f.read_text())
def digest(f):return hashlib.sha256(f.read_bytes()).hexdigest()
def files(base,index):
 for v in index:
  f=(base/v['path']).resolve();assert f.is_relative_to(base) and f.stat().st_size==v['bytes'] and digest(f)==v['sha256']
tracked=set(subprocess.check_output(['git','ls-files','--',str(p.relative_to(root))],cwd=root,text=True,timeout=10).splitlines());index=read(p/'files.json');files(p,index);assert all(str((p/v['path']).relative_to(root)) in tracked for v in index)
e=read(p/'evidence.json');assert e['selectedRustPassed']==39 and e['baselineCharacterizationPassed']==1 and e['strictClippy'] and not e['newDependenciesOrFeatures'] and not e['productionAdmission'];a=read(p/'artifacts.json');assert len(a)==e['archiveOriginals']==26;files(raw,a)
groups=read(p/'cleanup.json');assert len(groups)==e['ownedGroups']==len({v['pgid'] for v in groups})==20 and all(v['independentlyAbsent'] for v in groups)
receipts=read(p/'receipts.json');assert set(receipts)=={'current','baseline'};counts={}
for label,v in receipts.items():
 assert v==read(raw/label/'results.json') and v['source']==e['source' if label=='current' else 'baselineSource'] and v['terminal'] and v['exitCode']==0
 assert digest(raw/(label+'-helper.py'))==v['helperSha256'] and digest(raw/'bounded-runner.py')==v['runnerSha256']
 for step in v['steps']:
  assert step['exitCode']==step['childExitCode']==0 and step['groupGone'] and not step['cleanupSignals'] and not step['timedOut'] and step['sourceCleanBefore'] and step['sourceCleanAfter'];log=raw/label/step['privateLog'];assert digest(log)==step['logSha256']
  m=re.search(r'test result: ok\. (\d+) passed; 0 failed; 0 ignored;',log.read_text())
  if m:counts[label+'/'+step['name']]=int(m[1])
 assert any(step['name']=='media-tests-strict-clippy' for step in v['steps'])
assert counts==e['counts'] and sum(v for k,v in counts.items() if k.startswith('current/'))==39 and counts['baseline/sparse-capacity-baseline']==1
def fields(row):return {k:(int(v) if v.isdigit() else v) for k,v in re.findall(r'([a-z_]+)=([A-Za-z_]+|[0-9]+)',row)}
def rows(label,name,prefix):return [fields(row) for row in (raw/label/'private'/(name+'.log')).read_text().splitlines() if row.lstrip('.').startswith(prefix+' ')]
base=rows('baseline','sparse-capacity-baseline','sparse_capacity_baseline');assert len(base)==1;b=base[0]
assert b['small_packets']==3 and b['interval_ms']==900 and b['feedback_age_us']<100000 and b['held_kbps']==1500 and b['large_would_admit']=='true' and b['request_probe']=='false' and b['receiver_us']>2_000_000 and b['queue_max_us']>1_500_000 and b['overflow']==0
fixed=rows('current','sparse-network','sparse_capacity_fixed');assert len(fixed)==2 and {v['capacity_bps'] for v in fixed}=={250000,3000000}
for v in fixed:
 assert v['small_packets']==3 and v['interval_ms']==900 and v['feedback_age_us']<100000 and v['request_probe']=='true' and v['padding_packets']==32 and v['padding_rtp_bytes']==8704 and 0<v['probe_us']<500000 and 0<v['total_us']<750000 and v['queue_max_us']<750000 and v['overflow']==0
 assert v['resume_bytes']<=v['native_budget_bytes']*4//5
 assert (v['resume_bytes']==64005 if v['capacity_bps']==3000000 else 5<v['resume_bytes']<64005)
dense=rows('current','dense-network','dense_capacity_fixed');assert len(dense)==1;v=dense[0];assert v['frames']==13 and v['active_us']>2_000_000 and v['normal_cap_bps']>2031905 and v['padding_packets']==0 and v['resume_bytes']==64005 and v['queue_max_us']<750000 and v['overflow']==0
# Frozen earlier proofs deliberately remain attributable only to their source.
subprocess.run(['git','diff','--exit-code',e['source'],'--','media/src','media/vendor/rtc/src','media/Cargo.toml','media/Cargo.lock','Cargo.toml','Cargo.lock'],cwd=root,check=True,timeout=15)
print('Verified sparse baseline defect; 39 current tests/strict Clippy; bounded slow and identical-payload fast refresh, dense no-padding path; 26 originals and 20 absent groups. No native quality/resources or production admission.')
