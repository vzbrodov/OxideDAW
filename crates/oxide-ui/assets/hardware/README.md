# Original hardware controls

`rotary.png`, `pad-up.png` and `pad-down.png` are original procedural renders
made with Blender 5.2 / Cycles. They use modelled elastomer, anodised aluminium,
a turned metal cap and three area lights. No photography, external textures or
third-party product artwork is included.

Rebuild from this directory with `blender -b -P render.py`. The generator writes
transparent PNGs next to itself. The application embeds these files and loads
textures once; labels, indicators, focus states and interactions remain native
vector drawing. The knob illumination stays fixed while the index rotates.

The source and generated assets use the repository's MIT OR Apache-2.0 license.
