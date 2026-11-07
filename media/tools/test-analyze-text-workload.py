#!/usr/bin/env python3
"""Small metric contracts only; no images, codecs or OS input are exercised."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("text_workload", Path(__file__).with_name("analyze-text-workload.py"))
metrics = importlib.util.module_from_spec(spec)
spec.loader.exec_module(metrics)


class TextMetricsTests(unittest.TestCase):
    def test_fixture_cadence_requires_consistent_clock_counts(self):
        for fps,total,prefix,cycle in [(30,270,30,240),(60,540,60,480)]:
            manifest={"schemaVersion":1,"fps":fps,"totalFrames":total,"warmupFrames":prefix,"observedFrames":cycle}
            self.assertEqual(metrics.clock(manifest),{"fps":fps,"total":total,"prefix":prefix,"cycle":cycle})
            for field in ["fps","totalFrames","warmupFrames","observedFrames"]:
                changed=manifest|{field:True}
                with self.assertRaises(ValueError):metrics.clock(changed)
            with self.assertRaises(ValueError):metrics.clock(manifest|{"fps":120})
        with self.assertRaises(ValueError):metrics.clock({"schemaVersion":1,"fps":60,"totalFrames":270,"warmupFrames":30,"observedFrames":240})

    @unittest.skipUnless(importlib.util.find_spec("numpy"), "NumPy engine is optional")
    def test_vectorized_errors_equal_scalar_including_255_differences(self):
        original = bytes([0,255,0,255,0,255,16,80,200,200,80,16])
        frames = [bytes(255-value for value in original), original, bytes([0]*12)]
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"decoded.rgb"
            path.write_bytes(b"".join(frames))
            scalar = metrics.compare(path,original,[0,1,3],3,900,"python")
            vector = metrics.compare(path,original,[0,1,3],3,900,"numpy")
            self.assertEqual(scalar,vector)
            self.assertEqual(scalar["rows"][0]["maxAbsoluteForegroundCodeError"],255)
            self.assertEqual(scalar["rows"][1]["sumAbsoluteCodeErrorByRgbChannel"],[0,0,0])

    def test_cyclic_source_position_does_not_repeat_capture_clock(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"decoded.rgb"
            path.write_bytes(bytes([10,20,30])*241)
            value = metrics.compare(path,bytes([10,20,30]),[0],241,900)
            self.assertEqual([(v["sourceFrame"],v["captureFrame"])for v in value["rows"][-2:]],[(269,1139),(30,1140)])
            self.assertEqual(value["frames"],241)
            self.assertEqual(value["frameMeanErrorP95"],0)

    def test_sixty_fps_source_cycle_preserves_capture_clock(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/"decoded.rgb";path.write_bytes(bytes([10,20,30])*481)
            value=metrics.compare(path,bytes([10,20,30]),[0],481,1800,source_prefix=60,source_cycle=480)
            self.assertEqual([(row["sourceFrame"],row["captureFrame"])for row in value["rows"][-2:]],[(539,2279),(60,2280)])
            self.assertEqual(value["frameMeanErrorP95"],0)

    def test_partial_and_linked_decoded_data_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)/"decoded.rgb"
            path.write_bytes(b"12345")
            with self.assertRaisesRegex(ValueError,"frame count"):
                metrics.compare(path,b"123",[0],2)
            link = Path(folder)/"linked.rgb"
            link.symlink_to(path)
            with self.assertRaisesRegex(ValueError,"non-symlink"):
                metrics.compare(link,b"123",[0],2)

    def test_channel_regression_cannot_hide_behind_lower_total_error(self):
        row = lambda values: {"sumAbsoluteCodeErrorByRgbChannel":values}
        baseline = [row([10,10,10]),row([20,20,20])]
        candidate = [row([11,5,5]),row([19,19,19])]
        value = metrics.nonregression(candidate,baseline)
        self.assertTrue(value["sameOrLowerAggregateForegroundErrorEveryFrame"])
        self.assertFalse(value["sameOrLowerAggregateChannelErrorEveryFrame"])
        self.assertEqual(value["foregroundChannelErrorWorseThanBaselineFrames"],{"r":[0],"g":[],"b":[]})
        with self.assertRaisesRegex(ValueError,"frame counts"):
            metrics.nonregression(candidate[:1],baseline)


if __name__ == "__main__":
    unittest.main()
