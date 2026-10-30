# Import fixtures: sources and licences

## After Effects projects (`aep_*.aep`)

Projects saved by After Effects, from the sample corpus of **py-aep**
(<https://github.com/forticheprod/py-aep>, formerly `forticheprod/aep_parser`),
commit `18b52f1a52b1c7a9ed9e715813c912fe986b9ef3`. Licence: **MIT**,
Copyright (c) 2023 Fortiche production. Files renamed, content unchanged.

| Fixture | Source path in py-aep | SHA-256 |
|---|---|---|
| `aep_complete_ae2025.aep` | `samples/versions/ae2025/complete.aep` (AE 25, file version 96.9) | `489a04947a2f1d1124606643fecbbdea653a500e24f2e46ae450f601ada209e8` |
| `aep_ease_scale.aep` | `samples/models/property/keyframe_bezier_ease_scale.aep` | `85d90b672c3d40524ad273785902e167d9246936aef156d3e0e3e098fada975f` |
| `aep_eg_controllers.aep` | `samples/models/essential_graphics/multiple_controllers.aep` | `df08145c4be5d3547b1bb5650b48be777f8663d0d88dc472f65990f0ac37c6d3` |
| `aep_eg_media.aep` | `samples/models/essential_graphics/media_replacement.aep` | `dcb61584658d6ec02fa9239c2035a9e7a0e218d367935949166a97ba0d9313e4` |
| `aep_eg_text.aep` | `samples/models/essential_graphics/text_source_text.aep` | `3d6e2e54b993116f95221249e9f1cd309aa80e15e02e435a1601352cdaddc2df` |

The `.mogrt` and « Collect Files » archives used by the tests are built by
the tests themselves from these projects (with a hand-written
`definition.json` and a generated PNG).

py-aep licence (applies to the files above):

```
MIT License

Copyright (c) 2023 Fortiche production

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
