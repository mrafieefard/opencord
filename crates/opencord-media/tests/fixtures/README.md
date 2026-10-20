# Test Fixtures

Generated using system ffmpeg:

```bash
ffmpeg -f lavfi -i testsrc=size=320x240:rate=25 -f lavfi -i sine=frequency=440:sample_rate=48000 -t 2 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 64k -movflags +faststart h264_aac.mp4
ffmpeg -f lavfi -i testsrc=size=320x240:rate=25 -f lavfi -i sine=frequency=440:sample_rate=48000 -t 2 -c:v libvpx-vp9 -b:v 200k -c:a libopus vp9_opus.webm
ffmpeg -display_rotation 90 -i h264_aac.mp4 -c copy rotated_90.mp4
```
