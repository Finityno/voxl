import subprocess,os,pathlib,signal,json,time,sys
root=pathlib.Path('/Users/sondre/Documents/GitHub/voxl-desktop-native-validation');logs=pathlib.Path('/tmp/fincode-desktop-codec-76adf15')
env=dict(os.environ,DEVELOPER_DIR='/Applications/Xcode.app/Contents/Developer');expected='76adf15ee3d67541a229b3f6c9ef4b143f915d5e';steps=[]
def run(label,cmd,seconds):
 started=time.monotonic()
 with (logs/(label+'.log')).open('w') as out:
  p=subprocess.Popen(cmd,cwd=root,env=env,stdout=out,stderr=subprocess.STDOUT,start_new_session=True)
  try: code=p.wait(timeout=seconds)
  except BaseException:
   os.killpg(p.pid,signal.SIGTERM)
   try:p.wait(timeout=10)
   except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
   raise
 steps.append(dict(step=label,command=cmd,exit_code=code,seconds=round(time.monotonic()-started,2)))
 name='native-results.json' if '--locked' in sys.argv else 'queue-results.json'
 (logs/name).write_text(json.dumps(dict(head=expected,steps=steps),indent=2));print(label,code,flush=True);return code
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==expected
assert not subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True).strip()
if '--locked' not in sys.argv:raise SystemExit(run('queue',['/usr/bin/lockf','-k','-t','1800','/tmp/fincode-build.lock','/usr/bin/nice','-n','19',sys.executable,__file__,'--locked'],3000))
run('environment',['/bin/sh','-c','uptime; df -h .; cargo --version'],30)
base=['cargo','--locked','-q']
if run('test-build',['cargo','test','--locked','--manifest-path','media/Cargo.toml','--lib','--no-run','-j','2'],600):raise SystemExit(1)
if run('tests',['cargo','test','--locked','--manifest-path','media/Cargo.toml','--lib','avcc_rejects_truncation','-j','2'],60):raise SystemExit(1)
if run('clippy',['cargo','clippy','--locked','--manifest-path','media/Cargo.toml','--lib','--examples','-j','2','-q','--message-format=short','--','-D','warnings'],300):raise SystemExit(1)
if run('release-build',['cargo','build','--locked','--manifest-path','media/Cargo.toml','--release','--example','codec_probe','-j','2','-q','--message-format=short'],600):raise SystemExit(1)
reference=logs/'reference.nv12'
with reference.open('wb') as out:
 for frame in range(60):
  rows=[]
  for row in range(720):rows.append(bytes(200 if (col+frame*8)%1280<32 else 40 if row%24<2 and col%12<8 else 100 for col in range(1280)))
  out.write(b''.join(rows));out.write(bytes([128])*(1280*360))
for mode in ['hardware','software']:
 target=str(logs/(mode+'.h264'))
 if run(mode,['/usr/bin/time','-lp',str(root/'media/target/release/examples/codec_probe'),mode,target],30):continue
 run(mode+'-decode',['/opt/homebrew/bin/ffprobe','-v','error','-count_frames','-select_streams','v:0','-show_entries','stream=codec_name,width,height,nb_read_frames','-of','json',target],30)
 run(mode+'-quality',['/opt/homebrew/bin/ffmpeg','-nostdin','-hide_banner','-threads','2','-filter_complex_threads','1','-f','rawvideo','-pixel_format','nv12','-video_size','1280x720','-framerate','30','-i',str(reference),'-threads','2','-framerate','30','-i',target,'-lavfi','[0:v]format=yuv420p[r];[1:v]format=yuv420p[d];[r][d]ssim','-f','null','-'],60)
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==expected
assert not subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True).strip()
