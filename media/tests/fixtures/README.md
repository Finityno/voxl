# AV1 low-delay synthetic fixture

`av1-main-low-delay.ivf` is an original generated test signal, with no captured
screen or user content. It has 20 320×180 YUV420 8-bit Main frames at 10 fps,
with random-access units at frames 0 and 10. It is used for parser, fragmented
RTP and decoder interoperability checks; it is not a desktop quality, bandwidth
comparison, CPU fallback, encoder-latency or hardware qualification workload.

SHA-256: `1d41f651896c97264f62f2372ffabc8a916acf8b6cf47140568efa47411f8baa`.
`av1-main-low-delay.md5` contains the independent libdav1d decoded frame hashes.
SVT-AV1 4.2.0 maps requested preset 12 to 11 and reports low delay, CQP 40,
8-bit YUV420, two-way parallelism, GOP 10. The coded height is padded to 184;
the displayed/cropped frame is 320×180. Its startup display-delay metadata is
bounded codec data, not a new transport buffer or a latency measurement.

Generation (under nice 19, two jobs and an external process-group deadline):

```sh
ffmpeg -nostdin -hide_banner -v info \
  -f lavfi -i testsrc2=size=320x180:rate=10 -frames:v 20 -an \
  -c:v libsvtav1 -threads 2 -preset 12 -crf 40 -g 10 \
  -svtav1-params lp=2:pred-struct=1:lookahead=0:enable-overlays=0 \
  -pix_fmt yuv420p -f ivf av1-main-low-delay.ivf
ffmpeg -nostdin -hide_banner -v info -threads 2 \
  -i av1-main-low-delay.ivf -threads 2 -f framemd5 av1-main-low-delay.md5
```

The fixture is dedicated to the public domain under CC0-1.0. The generator,
codec implementation and transport specifications retain their own licenses.
