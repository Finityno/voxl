#!/usr/bin/env python3
"""Check the retained native support receipt without running native code."""
from pathlib import Path
import gzip,hashlib,json,re
root=Path(__file__).resolve().parent
index=json.loads((root/'files.json').read_text())
for name,expected in index['files'].items():
    path=root/name
    assert path.is_relative_to(root) and hashlib.sha256(path.read_bytes()).hexdigest()==expected,name
receipt=json.loads((root/'receipt.json').read_text());support=json.loads((root/'support.json').read_text())
assert receipt['source']==support['source']==index['source']=='4391a67750a492aaa0dc97c4f30e4a31225b7076'
assert receipt['terminal'] and receipt['exitCode']==0 and len(receipt['steps'])==index['independentlyAbsentGroups']==21
negative={f'{codec}-delay-{limit}' for codec in ['h264','hevc'] for limit in [0,1]}
steps={step['name']:step for step in receipt['steps']};assert len(steps)==21
for name,step in steps.items():
    assert step['groupGone'] and not step['timedOut'] and not step['cleanupSignals'] and step['sourceCleanBefore'] and step['sourceCleanAfter']
    assert step['exitCode']==(1 if name in negative else 0)
    assert step['childExitCode']==step['exitCode']
    if name in negative:
        log=gzip.decompress((root/'logs'/(name+'.log.gz')).read_bytes()).decode()
        assert 'Error: Platform { operation: "maximum frame delay", code: -12900 }' in log and 'delay_property_verified=true' not in log
assert len(support['cases'])==6
for codec in ['h264','hevc']:
    rows=[v for v in support['cases'] if v['codec']==codec];assert [v['requested'] for v in rows]==[None,0,1]
    assert rows[0]['supported'] and rows[0]['decodedFrames']==14 and not rows[0]['nativePropertyVerified']
    assert all(not v['supported'] and v['exitCode']==1 for v in rows[1:])
    decoded=json.loads(gzip.decompress((root/'logs'/(codec+'-delay-default-decode.log.gz')).read_bytes()))
    assert decoded['streams']==[{'codec_name':codec,'width':320,'height':180,'nb_read_frames':'14'}]
for name,count in [('maximum_frame_delay_',2),('output_wake_publishes_before_signaling_and_signals_only_the_first_failure',1)]:
    log=gzip.decompress((root/'logs'/(name+'.log.gz')).read_bytes()).decode()
    assert re.search(rf'test result: ok\. {count} passed; 0 failed; 0 ignored;',log)
assert receipt['productionAdmission'] is False and support['productionAdmission'] is False
print('Verified 3 selected Rust tests, 2 decoded controls (28 frames), 4 unsupported native requests and 21 clean terminated groups. No performance admission.')
