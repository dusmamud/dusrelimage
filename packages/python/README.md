# dusrelimage (Python) — thin wrapper

Source-tree wrapper around the compiled `dusrelimage` extension.
The image work lives in the Rust core (`crates/dusrelimage`),
reached through `dusrelimage._native` (shipped by the maturin wheel
built from `crates/dusrelimage-py`); this package re-exports those
functions with the same `DusrelError` translation, accepting an
`options` dict or keyword arguments (kwargs win).

```sh
pip install dusrelimage
```

```python
import dusrelimage

small = dusrelimage.compress_to_size(png_bytes, 50 * 1024)
thumb = dusrelimage.resize_image(png_bytes, width=800)
```

See `crates/dusrelimage-py/README.md` for the full operation list,
error codes and honest limits. Pydantic v2 models live in
`dusrelimage.pydantic` (requires `pip install dusrelimage[pydantic]`).
