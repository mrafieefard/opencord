# Test recordings

Recordings used by the voice processing tests (Phase 2 plan §7.2–§7.4, V3). Each is a short excerpt of a recording on Wikimedia Commons, converted with FFmpeg to mono 16-bit WAV.

| File | Excerpt | Source | Author | Licence |
|---|---|---|---|---|
| `speech.wav` (22.05 kHz) | 0:00.65–0:04.15 | [LibriVox - Winnie-the-Pooh (1926) 00.mp3](https://commons.wikimedia.org/wiki/File:LibriVox_-_Winnie-the-Pooh_(1926)_00.mp3) | LibriVox volunteers | Public domain |
| `noise/keyboard.wav` (24 kHz) | 0:10–0:15 | [Laptop keyboard.ogg](https://commons.wikimedia.org/wiki/File:Laptop_keyboard.ogg) (PDSounds no. 554) | russiandoll | Public domain |
| `noise/fan.wav` (24 kHz) | 0:28–0:33 | [Table ventilator.ogg](https://commons.wikimedia.org/wiki/File:Table_ventilator.ogg) (PDSounds no. 266) | stephan | Public domain |
| `noise/street.wav` (24 kHz) | 0:12–0:17 | [Sunday in the city street noise1.ogg](https://commons.wikimedia.org/wiki/File:Sunday_in_the_city_street_noise1.ogg) (PDSounds no. 530) | cori | Public domain |
| `noise/dog.wav` (24 kHz) | the whole recording, played twice | [Barking of a dog.ogg](https://commons.wikimedia.org/wiki/File:Barking_of_a_dog.ogg) | Amada44 | [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/) |

`noise/dog.wav` is an adaptation of Amada44's recording and is shared under the same licence, CC BY-SA 3.0. The other files are in the public domain.

# Test videos

Used by the media player tests (`tests/player.rs`) and the core's video upload test (media uploads plan, Task 33). Made once with the system `ffmpeg` command from its test pattern and a sine tone, so they carry no third-party content:

```bash
ffmpeg -f lavfi -i testsrc=size=320x240:rate=25 -f lavfi -i sine=frequency=440:sample_rate=48000 -t 2 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 64k -movflags +faststart h264_aac.mp4
ffmpeg -f lavfi -i testsrc=size=320x240:rate=25 -f lavfi -i sine=frequency=440:sample_rate=48000 -t 2 -c:v libvpx-vp9 -b:v 200k -c:a libopus vp9_opus.webm
ffmpeg -display_rotation 90 -i h264_aac.mp4 -c copy rotated_90.mp4
```
