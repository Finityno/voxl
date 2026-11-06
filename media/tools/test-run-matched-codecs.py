#!/usr/bin/env python3
"""Integrity contracts for codec evidence parsers; no native codec is launched."""
import importlib.util
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest

spec = importlib.util.spec_from_file_location("matched_codecs",Path(__file__).with_name("run-matched-codecs.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class CodecEvidenceTests(unittest.TestCase):
    def test_reusable_geometry_keeps_the_legacy_default_and_exact_4k_source_shape(self):
        defaults=runner.Runner(SimpleNamespace(workspace=Path('/tmp/unused'),developer_dir=None))
        self.assertIn('1920x1080',defaults.source_args());self.assertEqual(defaults.fps,30)
        high=runner.Runner(SimpleNamespace(workspace=Path('/tmp/unused'),developer_dir=None,width=3840,height=2160,fps=60))
        self.assertIn('3840x2160',high.source_args());self.assertIn('60',high.source_args())

    def test_pipeline_evidence_requires_bounded_fifo_counts_and_advancing_receipt_clock(self):
        text='pipeline_depth=2 maximum_pending=2 submitted_frames=4 completed_frames=4 output_order=verified\noutput_arrivals_ms=[20.0, 40.0, 60.0]\nsubmission_lateness_ms=[0.0, 1.0, 2.0]\n'
        value=runner.pipeline_evidence(text,2,4,3)
        self.assertEqual(value['observedCompletionFps'],50.0);self.assertEqual(value['maximumPending'],2)
        for bad in [text.replace('maximum_pending=2','maximum_pending=3'),text.replace('completed_frames=4','completed_frames=3'),text.replace('20.0, 40.0, 60.0','20.0, 20.0, 60.0'),text.replace('[0.0, 1.0, 2.0]','[0.0, 1.0]'),text.replace('[0.0, 1.0, 2.0]','[0.0, True, 2.0]')]:
            with self.assertRaises(ValueError):runner.pipeline_evidence(bad,2,4,3)

    def test_profile_priority_is_explicit_without_default_alias_or_codec_substitution(self):
        self.assertEqual(runner.profile("h264:compatible:3000"),("h264","compatible",3000,"default",None))
        self.assertEqual(runner.profile("hevc:low-latency-compatible:1500:speed"),("hevc","low-latency-compatible",1500,"speed",None))
        self.assertEqual(runner.profile("h264:constrained-high:3000:quality"),("h264","constrained-high",3000,"quality",None))
        for profile in ["h264:compatible:3000:default","hevc:constrained-high:1500:speed","av1:compatible:1200:speed","h264:compatible:0:speed","h264:compatible:3000:speed:extra"]:
            with self.assertRaises(ValueError):
                runner.profile(profile)

    def test_maximum_qp_is_bounded_explicit_and_does_not_replace_the_requested_priority(self):
        self.assertEqual(runner.profile("hevc:low-latency-compatible:12000:qp28"),("hevc","low-latency-compatible",12000,"default",28))
        self.assertEqual(runner.profile("h264:compatible:3000:speed:qp51"),("h264","compatible",3000,"speed",51))
        for profile in ["h264:compatible:3000:qp0","h264:compatible:3000:qp52","h264:compatible:3000:qp01","h264:compatible:3000:qp28:qp32","h264:compatible:3000:default:qp28","h264:compatible:3000:qp28:speed","hevc:compatible:24000:qp28"]:
            with self.assertRaises(ValueError):runner.profile(profile)

    def stream(self, codec="h264", wrong_clock=False, wrong_flag=False, fps=2, total=8):
        result = bytearray(b"VXFR\x01"+bytes([0 if codec == "h264" else 1]))
        for index in range(total):
            key = index%(fps*2) == 0
            types = ([7,8,5] if key else [1]) if codec == "h264" else ([32,33,34,19] if key else [1])
            payload = b"".join(b"\0\0\0\1"+bytes([value if codec == "h264" else value<<1])+b"\x80" for value in types)
            pts = index*1_000_000//fps+(1 if wrong_clock and index == 5 else 0)
            flag = int(key)^(1 if wrong_flag and index == 5 else 0)
            result += pts.to_bytes(8,"big",signed=True)+bytes([flag])+len(payload).to_bytes(4,"big")+payload
        return bytes(result)

    def test_access_units_separate_warmup_and_observed_payload(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"frames.vxfr"
            for codec in ["h264","hevc"]:
                path.write_bytes(self.stream(codec))
                value = runner.access_units(path,codec,8,2,2)
                self.assertEqual(value["keyframeIndices"],[0,4])
                self.assertEqual(value["observedFrames"],6)
                self.assertEqual(value["observedAnnexBBytes"],sum(value["observedFrameBytes"]))
                self.assertEqual(value["firstFrameAnnexBBytes"],18 if codec == "h264" else 24)
                self.assertAlmostEqual(value["meanAnnexBMbps"],(48 if codec == "h264" else 54)*8/3/1_000_000)

    def test_partial_extra_and_misclocked_streams_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"frames.vxfr"
            for data in [self.stream()[:12],self.stream()[:-1],self.stream()+b"x",self.stream(wrong_clock=True),self.stream(wrong_flag=True)]:
                path.write_bytes(data)
                with self.assertRaises(ValueError):
                    runner.access_units(path,"h264",8,2,2)

    def test_sixty_fps_clock_and_one_second_payload_windows(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/"frames.vxfr"
            path.write_bytes(self.stream(fps=60,total=180))
            value=runner.access_units(path,"h264",180,60,60)
            self.assertEqual(value["keyframeIndices"],[0,120])
            self.assertEqual(value["observedFrames"],120)
            self.assertEqual(value["observedAnnexBBytes"],732)
            self.assertAlmostEqual(value["meanAnnexBMbps"],.002928)
            self.assertAlmostEqual(value["peakOneSecondAnnexBMbps"],.002976)
            with self.assertRaisesRegex(ValueError,"clock"):
                runner.access_units(path,"h264",180,60,30)

    def test_ssim_rows_preserve_channels_and_phase_boundaries(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"thin.ssim"
            path.write_text("\n".join(f"n:{index+1} Y:0.8 U:0.9 V:1.0 All:0.85 (8.2)" for index in range(241)))
            value = runner.parse_ssim(path,241)
            self.assertEqual(value["channels"]["V"]["mean"],1)
            self.assertEqual(value["channels"]["Y"]["phaseFrames"],{"still":61,"typing":60,"scrolling":60,"window-movement":60})
            self.assertAlmostEqual(value["channels"]["Y"]["minimum"],.8)

    def test_missing_nonfinite_and_wrong_count_ssim_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"thin.ssim"
            for data in ["n:1 Y:inf U:1 V:1 All:1","n:1 Y:0.8 U:0.9 All:0.8","n:2 Y:1 U:1 V:1 All:1"]:
                path.write_text(data)
                with self.assertRaises(ValueError):
                    runner.parse_ssim(path,1)
            path.write_text("n:1 Y:1 U:1 V:1 All:1")
            with self.assertRaisesRegex(ValueError,"count"):
                runner.parse_ssim(path,2)

    def test_sixty_fps_ssim_uses_two_second_phases(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/"phases.ssim"
            path.write_text("\n".join(f"n:{i+1} Y:{[.1,.2,.3,.4][i//120]} U:1 V:1 All:1"for i in range(480)))
            value=runner.parse_ssim(path,480,60)["channels"]["Y"]
            self.assertEqual(value["phaseFrames"],{"still":120,"typing":120,"scrolling":120,"window-movement":120})
            for key,expected in zip(["still","typing","scrolling","window-movement"],[.1,.2,.3,.4]):
                self.assertAlmostEqual(value["phaseMeans"][key],expected)


if __name__ == "__main__":
    unittest.main()
