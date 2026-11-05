#!/usr/bin/env python3
"""Integrity contracts for codec evidence parsers; no native codec is launched."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("matched_codecs",Path(__file__).with_name("run-matched-codecs.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class CodecEvidenceTests(unittest.TestCase):
    def stream(self, codec="h264", wrong_clock=False, wrong_flag=False):
        result = bytearray(b"VXFR\x01"+bytes([0 if codec == "h264" else 1]))
        for index in range(8):
            key = index%4 == 0
            types = ([7,8,5] if key else [1]) if codec == "h264" else ([32,33,34,19] if key else [1])
            payload = b"".join(b"\0\0\0\1"+bytes([value if codec == "h264" else value<<1])+b"\x80" for value in types)
            pts = index*1_000_000//2+(1 if wrong_clock and index == 5 else 0)
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


if __name__ == "__main__":
    unittest.main()
