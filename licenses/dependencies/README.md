# Checked-in dependency license evidence

Some crates in the locked runtime publish an SPDX license expression but omit
the referenced text from their crates.io archive. These copies come from the
corresponding upstream repository and are used only when the package archive
has no standalone license or notice file.

| Directory | Packages | Upstream revision |
|---|---|---|
| `alloc-stdlib` | alloc-stdlib 0.2.4 | dropbox/rust-alloc-no-stdlib `ae42d22078b98549e987d2f03d12df7b984fde47` |
| `objc2` | block2, dispatch2 and objc2 family | madsmtm/objc2 `7b1abfd750a2cacaea71d6a56ecfb83cb7de560b`; older locked crates use the same project grant |
| `defmt` | defmt-parser 1.0.0 | knurling-rs/defmt `4a8cdb44891ed57b8ff5a023b6bec7137c48708f` |
| `encoding-index` | encoding-index family | lifthrasiir/rust-encoding `eb3d3c307df864f6a25e2ca16d49703e5d963ec5` |
| `genpdf` | genpdf 0.2.0 | ireas/genpdf-rs `12fea24895b470ce9b7ff43487035cfaec234185`; Apache-2.0 selected from its `Apache-2.0 OR MIT` grant |
| `llama-cpp-2` | llama-cpp-2 0.1.156 | utilityai/llama-cpp-rs `63e549708237b16b39018288582655539e0a9d5b` |
| `selectors` | selectors 0.36.1 | servo/stylo `635e1a19d02960588a00e189bd4bd5bdb150ec3d`; MPL-2.0 text from Mozilla |
| `unic` | unic family | open-i18n/rust-unic `5878605364af97a3358368a6eaef02104af2e016` |

The generated inventory records the exact locked package version, declared
license, source and authors beside these texts.
