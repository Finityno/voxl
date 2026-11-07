from pathlib import Path
import hashlib,importlib.util,json,sys
ROOT=Path('$CHECKOUT');OUT=Path('$GENERATION_RESULTS');OLD=Path('$ORIGINAL_FIXTURE');A=OUT/'private/default30';B=OUT/'private/source60'
spec=importlib.util.spec_from_file_location('metrics',ROOT/'media/tools/analyze-text-workload.py');metrics=importlib.util.module_from_spec(spec);spec.loader.exec_module(metrics)
def sha(p):return hashlib.file_digest(p.open('rb'),'sha256').hexdigest()
original=json.loads((OLD/'manifest.json').read_text());default,_,_,default_check=metrics.reference(A);higher,_,_,higher_check=metrics.reference(B)
assert metrics.clock(default)=={'fps':30,'prefix':30,'cycle':240,'total':270}
assert metrics.clock(higher)=={'fps':60,'prefix':60,'cycle':480,'total':540}
controls={}
for name in ['manifest.json','reference.nv12','reference-thin.rgb']:
 a,b=sha(OLD/name),sha(A/name);assert a==b,(name,a,b);controls[name]={'originalSha256':a,'currentSha256':b,'byteIdentical':True}
assert [p['frames']for p in higher['phases']]==[120]*4 and [p['firstFrame']for p in higher['phases']]==[60,180,300,420]
size=higher['width']*higher['height']*3//2;motion=[]
with(B/'reference.nv12').open('rb')as stream:
 for frame in range(420,424):
  stream.seek(frame*size);data=stream.read(size);assert len(data)==size;motion.append(hashlib.sha256(data).hexdigest())
assert len(set(motion))==4,'60fps motion must not be duplicated 30fps frames'
summary={'source':'1f94f7618dc3e11abb15ad858112c8f4686a2320','default30ByteControls':controls,'default30Calibration':default_check,'source60Calibration':higher_check,'source60ManifestSha256':sha(B/'manifest.json'),'source60Clock':metrics.clock(higher),'consecutiveMotionFrameHashes':motion,'scope':'Fresh offscreen generated assets and independent source calibration only. No native encoder/decoder/capture/window/input or performance/profile admission. Source motion hashes show consecutive generated frames differ, not actual viewer presentation.'}
(OUT/'asset-validation.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))
