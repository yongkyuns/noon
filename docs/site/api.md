# API reference

Look up a symbol after learning the basic scene model. Python declarations and
Rustdoc are generated from the same source revision as this website.

## Choose a reference

| You are writing | Reference | Learn the model |
| --- | --- | --- |
| Python, in a browser or natively | [Python symbols](#python) | [Python walkthrough](python.html) |
| Rust | [Generated Rust API](rust/noon/index.html) | [Rust walkthrough](rust.html) |
| A native viewer | [Native host API](rust/noon_native/index.html) | [Rust library details](native-rust.html) |
| A native export tool | [Export host API](rust/noon_export/index.html) | [Exporting visuals](export.html) |

## Common starting points

[Scene](python-Scene.html) · [Mobject](python-Mobject.html) ·
[Circle](python-Circle.html) · [Square](python-Square.html) ·
[VGroup](python-VGroup.html) · [Text](python-Text.html) ·
[Create](python-Create.html) · [Transform](python-Transform.html)

## Rust

[Open generated Rustdoc](rust/noon/index.html) for public signatures, traits,
types and docstrings in this checkout's default feature profile. Each generated
page has a link back to this documentation. Host-specific setup is covered in
[the implementation reference](reference.html), not inferred from a symbol name.

## Python

Search below by class, function, constant, or source module. Each callable page
lists its declared signature, defaults, annotations, documentation and source.
Class pages identify bases and list declared members.

<!-- noon:python-index -->

## Coverage and host support

The index resolves `noon.__all__`, `_PUBLIC_EXPORTS`, and local aliases without
importing the runtime. Inherited or dynamically attached members are not expanded;
unresolved external/runtime exports are listed explicitly. Presence in this
reference is not a claim that every host and feature combination is supported.

For behavior and restrictions, start with [scene authoring](guide.html),
[playback](playback.html), or the [host references](reference.html).
