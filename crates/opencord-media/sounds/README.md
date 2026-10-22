# Default soundboard sounds

These are the sounds every Opencord client ships with (Phase 2 plan §11.8).
They are synthesized from oscillators and seeded noise by
`examples/default_sounds.rs`; no recordings are used. Running it again
writes the same files:

```sh
cargo run -p opencord-media --example default_sounds
```

To the extent possible under law, the Opencord contributors have waived all
copyright and related rights to these sound files
([CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/)).

The voice event sounds in `cues/` (Phase 2 §12) come from `examples/cue_sounds.rs` the same way, also CC0.
